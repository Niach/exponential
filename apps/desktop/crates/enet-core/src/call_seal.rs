//! #EXP-1941: the call seal (packages/etalk/fixtures/voice-call.json `seal` is
//! the spec and holds the vectors; TS reference:
//! packages/domain-contract/src/call-seal.ts). Two sealing ends of a Voice
//! call prove that both hold the chat's tree SECRET `S` and seal the call's
//! audio and events under keys of a fresh X25519 exchange, so the relay
//! forwards bytes it cannot read. The order is Noise NNpsk2's.
//!
//! ```text
//! caller hello = 0x01 ‖ role (1 caller, 2 host) ‖ epoch u32 BE ‖ rootId[16] ‖ eph[32]   (54)
//! host hello   = the same 54 with role 2 ‖ tag[16]                                       (70)
//! dh           = X25519(own eph secret, peer eph)            (all-zero = refused)
//! transcript   = "exp/etalk/call/1" ‖ callId[16] ‖ agentId[16] ‖ SHA-256(utf8(deviceId))
//!                ‖ callerHello[54] ‖ hostHello[0..54]        (188 bytes; never the tag)
//! key(label)   = HKDF-SHA256(ikm = dh ‖ clamp(S), salt = transcript, info = label, L = 32)
//! labels       = "up/audio", "down/audio", "down/control", "down/hello"
//! tag          = ChaCha20-Poly1305(key("down/hello"), nonce = 00 × 12, aad = hostHello[0..54], empty)
//! frame        = ChaCha20-Poly1305(key, nonce = 00 × 8 ‖ ctr u32 BE, aad = the clear header)
//! ```
//!
//! The sealer is an OBJECT: it holds the keys and the counters and no method
//! takes a counter, so one nonce is never used twice. [`CallHandshake::finish`]
//! works once, a failed one too. A caller's `finish` fails unless the host's
//! tag opens and the hello names the root and epoch the caller's own server
//! answer names, so "a [`CallSeal`] exists" is the whole gate of a caller's
//! capture path. A host's [`CallSeal`] seals nothing before a caller frame
//! opened on it. Nothing here is wired into a call yet.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use chacha20poly1305::{aead::Aead, aead::Payload, ChaCha20Poly1305, KeyInit};
use curve25519_dalek::montgomery::MontgomeryPoint;
use hkdf::Hkdf;
use sha2::{Digest, Sha256};

use crate::sealed::{b64url_decode, b64url_encode};

pub const CALL_SEAL_VERSION: u8 = 1;
/// The device cap a host advertises while it has a key holder.
pub const CALL_SEAL_CAP: &str = "call-seal";
pub const CALLER_HELLO_BYTES: usize = 54;
pub const HOST_HELLO_BYTES: usize = 70;
const DOMAIN: &[u8] = b"exp/etalk/call/1";
const LABELS: [&str; 4] = ["up/audio", "down/audio", "down/control", "down/hello"];
const KIND_UP: u8 = 1;
const KIND_DOWN: u8 = 2;
const TYPE_AUDIO: u8 = 0;
const TYPE_HELLO: u8 = 1;
const HEADER_BYTES: usize = 6; // kind ‖ type ‖ ctr
const INNER_BYTES: usize = 5; // seq ‖ flags ‖ len
const TAG_BYTES: usize = 16;

/// Why the seal refused (the `reason` of `seal.refused`). Logs name it, never bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallSealRefusal {
    /// Not a hello of the other role, this version and its length.
    Hello,
    /// The host's hello names another root or epoch than the caller's own answer.
    Root,
    /// The exchange gave all zeros.
    LowOrder,
    /// A second `finish`.
    Finished,
    /// An id, a secret or a size of the wrong shape; a method of the other role.
    Input,
    /// A host's seal before any caller frame opened.
    Unproven,
    /// A packet above the step, a text above `max_plain`.
    TooLarge,
    /// The counter is used up.
    Exhausted,
    /// Not a frame of this channel: its size, kind, type or inner length.
    Frame,
    /// It does not open.
    Tag,
    /// An audio ctr not above the last accepted.
    Replay,
    /// A control ctr that is not exactly the next.
    Order,
}

impl CallSealRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::Root => "root",
            Self::LowOrder => "low_order",
            Self::Finished => "finished",
            Self::Input => "input",
            Self::Unproven => "unproven",
            Self::TooLarge => "too_large",
            Self::Exhausted => "exhausted",
            Self::Frame => "frame",
            Self::Tag => "tag",
            Self::Replay => "replay",
            Self::Order => "order",
        }
    }
}

impl std::fmt::Display for CallSealRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "call seal: {}", self.as_str())
    }
}

impl std::error::Error for CallSealRefusal {}

type Result<T> = std::result::Result<T, CallSealRefusal>;
use CallSealRefusal as No;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallRole {
    Caller,
    Host,
}

impl CallRole {
    fn byte(self) -> u8 {
        match self {
            Self::Caller => 1,
            Self::Host => 2,
        }
    }
    fn hello_bytes(self) -> usize {
        match self {
            Self::Caller => CALLER_HELLO_BYTES,
            Self::Host => HOST_HELLO_BYTES,
        }
    }
}

/// The sizes a sealer pads to (`seal.audioPadStep`, `controlPadBytes`,
/// `maxPlainBytes`): sender-only, a receiver trusts the frame's `len` alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallSealPad {
    pub audio_up: usize,
    pub audio_down: usize,
    pub control_pad: usize,
    pub max_plain: usize,
}

impl Default for CallSealPad {
    fn default() -> Self {
        Self { audio_up: 120, audio_down: 160, control_pad: 256, max_plain: 5888 }
    }
}

/// A hello's clear fields. `root` is all zeros from a caller that knew none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallHello {
    pub epoch: u32,
    pub root: [u8; 16],
    pub eph: [u8; 32],
}

/// "Well-formed": a hello of `role` by length, version and role byte. Pure.
pub fn parse_hello(hello: &[u8], role: CallRole) -> Result<CallHello> {
    if hello.len() != role.hello_bytes() || hello[0] != CALL_SEAL_VERSION || hello[1] != role.byte() {
        return Err(No::Hello);
    }
    Ok(CallHello {
        epoch: u32::from_be_bytes(hello[2..6].try_into().unwrap()),
        root: hello[6..22].try_into().unwrap(),
        eph: hello[22..54].try_into().unwrap(),
    })
}

/// The caller's hello on the wire: the binary frame kind `up` ‖ type hello ‖ hello.
pub fn hello_frame(hello: &[u8; CALLER_HELLO_BYTES]) -> Vec<u8> {
    [&[KIND_UP, TYPE_HELLO][..], hello].concat()
}

/// The hello of a binary frame, or None when the frame is none (a hello never goes down).
pub fn hello_of_frame(frame: &[u8]) -> Option<&[u8]> {
    (frame.len() == 2 + CALLER_HELLO_BYTES && frame[0] == KIND_UP && frame[1] == TYPE_HELLO).then(|| &frame[2..])
}

/// The host's hello on the wire: the `data` of `call_event {t: "hello"}`.
pub fn host_hello_text(hello: &[u8; HOST_HELLO_BYTES]) -> String {
    b64url_encode(hello)
}

pub fn host_hello_of_text(data: &str) -> Option<Vec<u8>> {
    b64url_decode(data).filter(|hello| hello.len() == HOST_HELLO_BYTES)
}

/// The 16 raw bytes of a UUID in either case.
pub fn uuid_bytes(id: &str) -> Result<[u8; 16]> {
    let text = id.as_bytes();
    if text.len() != 36 || [8, 13, 18, 23].iter().any(|&at| text[at] != b'-') {
        return Err(No::Input);
    }
    let digits: Vec<u8> = text.iter().copied().filter(|&c| c != b'-').collect();
    let bytes = data_encoding::HEXLOWER_PERMISSIVE.decode(&digits).map_err(|_| No::Input)?;
    bytes.try_into().map_err(|_| No::Input)
}

/// What `finish` needs besides the handshake's own eph secret.
pub struct CallFinish<'a> {
    /// The tree SECRET (32 bytes) of the HOST hello's root and epoch.
    pub secret: &'a [u8],
    /// Each end's OWN ids: the caller's `startCall` answer, the host's ring.
    pub call_id: &'a str,
    pub agent_id: &'a str,
    /// The host's steer device id.
    pub device_id: &'a str,
    /// The other end's hello: 54 bytes for a host, 70 for a caller.
    pub peer_hello: &'a [u8],
    /// CALLER ONLY: the root and the epoch its own `sendTarget` answer names
    /// NOW. A host hello that names others is refused (`root`).
    pub root_id: Option<&'a str>,
    pub epoch: u32,
    pub pad: CallSealPad,
}

struct Derived {
    dh: [u8; 32],
    transcript: Vec<u8>,
    /// By [`LABELS`]: up/audio, down/audio, down/control, down/hello.
    keys: [[u8; 32]; 4],
}

fn derive(eph: &[u8; 32], peer_eph: &[u8; 32], f: &CallFinish<'_>, caller_hello: &[u8], host_hello: &[u8]) -> Result<Derived> {
    let mut secret: [u8; 32] = f.secret.try_into().map_err(|_| No::Input)?;
    let transcript = [
        DOMAIN,
        &uuid_bytes(f.call_id)?,
        &uuid_bytes(f.agent_id)?,
        &Sha256::digest(f.device_id.as_bytes()),
        &caller_hello[..CALLER_HELLO_BYTES],
        &host_hello[..CALLER_HELLO_BYTES],
    ]
    .concat();
    let dh = MontgomeryPoint(*peer_eph).mul_clamped(*eph).to_bytes();
    if dh == [0u8; 32] {
        return Err(No::LowOrder);
    }
    // X25519 clamps at use: two byte strings that are one tree key give one call key.
    secret[0] &= 0xF8;
    secret[31] = (secret[31] & 0x7F) | 0x40;
    let mut ikm = [0u8; 64];
    ikm[..32].copy_from_slice(&dh);
    ikm[32..].copy_from_slice(&secret);
    let hk = Hkdf::<Sha256>::new(Some(&transcript), &ikm);
    let mut keys = [[0u8; 32]; 4];
    for (key, label) in keys.iter_mut().zip(LABELS) {
        hk.expand(label.as_bytes(), key).expect("32 bytes is a valid HKDF length");
    }
    secret.fill(0);
    ikm.fill(0);
    Ok(Derived { dh, transcript, keys })
}

fn nonce(ctr: u32) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[8..].copy_from_slice(&ctr.to_be_bytes());
    nonce
}

fn seal(key: &[u8; 32], ctr: u32, aad: &[u8], plain: &[u8]) -> Vec<u8> {
    ChaCha20Poly1305::new(key.into()).encrypt(&nonce(ctr).into(), Payload { msg: plain, aad }).expect("a frame is far below the cipher's limit")
}

fn open(key: &[u8; 32], ctr: u32, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
    ChaCha20Poly1305::new(key.into()).decrypt(&nonce(ctr).into(), Payload { msg: sealed, aad }).map_err(|_| No::Tag)
}

/// One end's half of the exchange. It holds the eph secret until `finish` takes it.
pub struct CallHandshake {
    role: CallRole,
    hello: [u8; CALLER_HELLO_BYTES],
    eph: Mutex<Option<[u8; 32]>>,
}

impl CallHandshake {
    /// A fresh eph key per socket. `root_id` None = a caller that knows no root yet (zeros).
    pub fn new(role: CallRole, root_id: Option<&str>, epoch: u32) -> Result<Self> {
        let mut eph = [0u8; 32];
        getrandom::fill(&mut eph).expect("system randomness");
        Self::with_eph_secret(role, root_id, epoch, eph)
    }

    /// VECTORS ONLY, so private to the crate: a fixed eph secret must never reach a call.
    pub(crate) fn with_eph_secret(role: CallRole, root_id: Option<&str>, epoch: u32, eph: [u8; 32]) -> Result<Self> {
        let root = root_id.map(uuid_bytes).transpose()?.unwrap_or_default();
        let mut hello = [0u8; CALLER_HELLO_BYTES];
        hello[0] = CALL_SEAL_VERSION;
        hello[1] = role.byte();
        hello[2..6].copy_from_slice(&epoch.to_be_bytes());
        hello[6..22].copy_from_slice(&root);
        hello[22..].copy_from_slice(&MontgomeryPoint::mul_base_clamped(eph).to_bytes());
        Ok(Self { role, hello, eph: Mutex::new(Some(eph)) })
    }

    pub fn role(&self) -> CallRole {
        self.role
    }

    /// The CALLER's hello: made once, resent unchanged. A host has no hello
    /// before it has the key (`input`): [`CallSeal::host_hello`] gives it.
    pub fn hello(&self) -> Result<[u8; CALLER_HELLO_BYTES]> {
        (self.role == CallRole::Caller).then_some(self.hello).ok_or(No::Input)
    }

    /// The keys of the call. WORKS ONCE: the eph secret is taken out and wiped
    /// whatever the answer, so a hello or a `call_ready` that comes again can
    /// never give a second sealer whose counters start at 0 under the same keys.
    pub fn finish(&self, input: &CallFinish<'_>) -> Result<CallSeal> {
        let mut eph = {
            let mut slot = self.eph.lock().unwrap_or_else(|e| e.into_inner());
            let eph = *slot.as_ref().ok_or(No::Finished)?;
            slot.as_mut().unwrap().fill(0);
            *slot = None;
            eph
        };
        let out = self.finish_with(&eph, input);
        eph.fill(0);
        out
    }

    fn finish_with(&self, eph: &[u8; 32], f: &CallFinish<'_>) -> Result<CallSeal> {
        let peer_role = if self.role == CallRole::Caller { CallRole::Host } else { CallRole::Caller };
        let peer = parse_hello(f.peer_hello, peer_role)?;
        let (caller_hello, host_hello) = if self.role == CallRole::Caller { (&self.hello[..], f.peer_hello) } else { (f.peer_hello, &self.hello[..]) };
        if self.role == CallRole::Caller && (uuid_bytes(f.root_id.ok_or(No::Input)?)? != peer.root || f.epoch != peer.epoch) {
            return Err(No::Root);
        }
        let Derived { mut dh, keys, .. } = derive(eph, &peer.eph, f, caller_hello, host_hello)?;
        dh.fill(0);
        let clear = &host_hello[..CALLER_HELLO_BYTES];
        let host_hello = match self.role {
            // The host's proof: it opens only under a key of `S`, this call's dh and both hellos.
            CallRole::Caller => open(&keys[3], 0, clear, &f.peer_hello[CALLER_HELLO_BYTES..]).map(|_| None)?,
            CallRole::Host => Some([clear, &seal(&keys[3], 0, clear, &[])].concat().try_into().expect("54 + 16 bytes")),
        };
        Ok(CallSeal {
            role: self.role,
            keys,
            pad: f.pad,
            host_hello,
            proven: AtomicBool::new(self.role == CallRole::Caller),
            send: Mutex::new((0, 0)),
            recv: Mutex::new((None, 0)),
        })
    }
}

/// An opened audio frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedAudio {
    /// The packet's number in its encoder run. Never a reason to refuse.
    pub seq: u16,
    /// Bit 0 on `up` = the microphone is muted; unknown bits pass.
    pub flags: u8,
    /// Empty for a frame with `len` 0: nothing of it reaches a decoder.
    pub packet: Vec<u8>,
    /// Frames missing before this packet: the ctr's gap, plus ONE for a frame
    /// with `len` 0 (it stands for a packet its sender could not seal). A
    /// receiver never joins audio across a gap.
    pub missing: u32,
}

/// The sealed call of ONE end: the caller seals `up` audio and opens `down`
/// audio and text, the host the reverse. It owns its counters; one object is
/// safe on two threads, one sealing and one opening.
pub struct CallSeal {
    role: CallRole,
    keys: [[u8; 32]; 4],
    pad: CallSealPad,
    host_hello: Option<[u8; HOST_HELLO_BYTES]>,
    /// A host seals nothing before a caller frame opened: the caller's proof.
    proven: AtomicBool,
    /// The next ctr this end seals with: (audio, control).
    send: Mutex<(u64, u64)>,
    /// The last audio ctr accepted, and the control ctr that must come next.
    recv: Mutex<(Option<u32>, u64)>,
}

impl CallSeal {
    pub fn role(&self) -> CallRole {
        self.role
    }

    /// HOST ONLY: its hello with the tag, sent once.
    pub fn host_hello(&self) -> Option<[u8; HOST_HELLO_BYTES]> {
        self.host_hello
    }

    fn sending(&self) -> Result<std::sync::MutexGuard<'_, (u64, u64)>> {
        if !self.proven.load(Ordering::Acquire) {
            return Err(No::Unproven);
        }
        Ok(self.send.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// One audio frame of this end's direction, always ONE size: 27 bytes plus
    /// the direction's step. A packet above the step is refused (`too_large`)
    /// and takes no counter: the sender seals an empty packet in its place.
    pub fn seal_audio(&self, seq: u16, flags: u8, packet: &[u8]) -> Result<Vec<u8>> {
        let up = self.role == CallRole::Caller;
        let step = if up { self.pad.audio_up } else { self.pad.audio_down };
        if step > u16::MAX as usize {
            return Err(No::Input);
        }
        let mut send = self.sending()?;
        if packet.len() > step {
            return Err(No::TooLarge);
        }
        let ctr = u32::try_from(send.0).map_err(|_| No::Exhausted)?;
        send.0 += 1;
        let mut plain = vec![0u8; INNER_BYTES + step];
        plain[..2].copy_from_slice(&seq.to_be_bytes());
        plain[2] = flags;
        plain[3..5].copy_from_slice(&(packet.len() as u16).to_be_bytes());
        plain[INNER_BYTES..INNER_BYTES + packet.len()].copy_from_slice(packet);
        let mut frame = vec![if up { KIND_UP } else { KIND_DOWN }, TYPE_AUDIO];
        frame.extend_from_slice(&ctr.to_be_bytes());
        let sealed = seal(&self.keys[if up { 0 } else { 1 }], ctr, &frame, &plain);
        frame.extend_from_slice(&sealed);
        Ok(frame)
    }

    /// Opens an audio frame of the OTHER direction. Only a ctr above the last
    /// accepted opens; gaps pass and are counted. A refused frame moves no mark.
    pub fn open_audio(&self, frame: &[u8]) -> Result<OpenedAudio> {
        let up = self.role == CallRole::Host;
        if frame.len() < HEADER_BYTES + INNER_BYTES + TAG_BYTES || frame[0] != if up { KIND_UP } else { KIND_DOWN } || frame[1] != TYPE_AUDIO {
            return Err(No::Frame);
        }
        let ctr = u32::from_be_bytes(frame[2..6].try_into().unwrap());
        let mut recv = self.recv.lock().unwrap_or_else(|e| e.into_inner());
        if recv.0.is_some_and(|seen| ctr <= seen) {
            return Err(No::Replay);
        }
        let plain = open(&self.keys[if up { 0 } else { 1 }], ctr, &frame[..HEADER_BYTES], &frame[HEADER_BYTES..])?;
        let len = u16::from_be_bytes([plain[3], plain[4]]) as usize;
        if len > plain.len() - INNER_BYTES {
            return Err(No::Frame);
        }
        let gap = ctr - recv.0.map_or(0, |seen| seen + 1);
        recv.0 = Some(ctr);
        self.proven.store(true, Ordering::Release);
        Ok(OpenedAudio {
            seq: u16::from_be_bytes([plain[0], plain[1]]),
            flags: plain[2],
            packet: plain[INNER_BYTES..INNER_BYTES + len].to_vec(),
            missing: gap.saturating_add(u32::from(len == 0)),
        })
    }

    /// HOST ONLY: one control frame of today as JSON text, padded with spaces
    /// to a multiple of `control_pad`. Gives the `data` of `call_event {t: "sealed"}`.
    pub fn seal_control(&self, text: &str) -> Result<String> {
        let CallSealPad { control_pad, max_plain, .. } = self.pad;
        if self.role != CallRole::Host || control_pad == 0 || control_pad > max_plain {
            return Err(No::Input);
        }
        let mut send = self.sending()?;
        let size = text.len().div_ceil(control_pad).max(1) * control_pad;
        if size > max_plain {
            return Err(No::TooLarge);
        }
        let ctr = u32::try_from(send.1).map_err(|_| No::Exhausted)?;
        send.1 += 1;
        let mut plain = vec![b' '; size];
        plain[..text.len()].copy_from_slice(text.as_bytes());
        let header = ctr.to_be_bytes();
        Ok(b64url_encode(&[&header[..], &seal(&self.keys[2], ctr, &header, &plain)].concat()))
    }

    /// CALLER ONLY: the text of a `sealed` event, trailing spaces included.
    /// The ctr is strict (the first is 0, then exactly last + 1): the relay
    /// never drops a text frame, so a gap is a deleted event and ends the call.
    pub fn open_control(&self, data: &str) -> Result<String> {
        if self.role != CallRole::Caller {
            return Err(No::Input);
        }
        let frame = b64url_decode(data).filter(|frame| frame.len() >= 4 + TAG_BYTES).ok_or(No::Frame)?;
        let ctr = u32::from_be_bytes(frame[..4].try_into().unwrap());
        let mut recv = self.recv.lock().unwrap_or_else(|e| e.into_inner());
        if u64::from(ctr) != recv.1 {
            return Err(No::Order);
        }
        let text = String::from_utf8(open(&self.keys[2], ctr, &frame[..4], &frame[4..])?).map_err(|_| No::Frame)?;
        recv.1 += 1;
        Ok(text)
    }
}

mod replay;
pub use replay::selftest;

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../../../packages/etalk/fixtures/voice-call.json");

    /// Every vector of `seal` byte for byte, every entry of `refused` refused.
    #[test]
    fn replays_the_call_seal_fixture() {
        selftest(FIXTURE).unwrap_or_else(|failed| panic!("{failed}"));
    }

    /// The replay is not vacuous: one changed byte of the fixture is named.
    #[test]
    fn the_replay_names_what_fails() {
        let tag = "021df2d28ad2384e35f042fbb30b425d";
        let failed = selftest(&FIXTURE.replace(tag, &tag.replace("021d", "031d"))).unwrap_err();
        assert!(failed.contains("derived: hostHelloTag") && failed.contains("the host's hello with its tag"), "{failed}");
        let failed = selftest(&FIXTURE.replace(r#""reason": "unproven""#, r#""reason": "tag""#)).unwrap_err();
        assert_eq!(failed.lines().count(), 2, "{failed}");
        assert!(selftest("{}").is_err() && selftest("no json").is_err());
    }

    /// A live call on fresh keys: a first call (the caller knew no root),
    /// hello for hello with its tag, a packet up, a caption down.
    #[test]
    fn a_live_call_on_fresh_keys() {
        let root = "11111111-1111-4111-8111-111111111111";
        let secret = [7u8; 32];
        let (caller_side, host_side) = (CallHandshake::new(CallRole::Caller, None, 0).unwrap(), CallHandshake::new(CallRole::Host, Some(root), 3).unwrap());
        assert_eq!(host_side.hello().err(), Some(No::Input), "a host has no hello before it has the key");
        assert_ne!(CallHandshake::new(CallRole::Caller, None, 0).unwrap().hello().unwrap(), caller_side.hello().unwrap());
        let input = |peer_hello| CallFinish {
            secret: &secret,
            call_id: "22222222-2222-4222-8222-222222222222",
            agent_id: "33333333-3333-4333-8333-333333333333",
            device_id: "desk-7f3a",
            peer_hello,
            root_id: Some(root),
            epoch: 3,
            pad: CallSealPad::default(),
        };
        let caller_hello = caller_side.hello().unwrap();
        let host = host_side.finish(&input(&caller_hello)).unwrap();
        let host_hello = host.host_hello().unwrap();
        let caller = caller_side.finish(&input(&host_hello)).unwrap();
        assert!(caller.host_hello().is_none());

        let up = caller.seal_audio(9, 1, &[0xf8, 0xff, 0xfe]).unwrap();
        assert_eq!(up.len(), 27 + 120);
        assert_eq!(host.open_audio(&up).unwrap(), OpenedAudio { seq: 9, flags: 1, packet: vec![0xf8, 0xff, 0xfe], missing: 0 });
        let down = host.seal_audio(u16::MAX, 0, &[7; 160]).unwrap();
        assert_eq!((down.len(), caller.open_audio(&down).unwrap().packet.len()), (27 + 160, 160));
        let caption = r#"{"t":"call_event","event":{"t":"partial","text":"grüß dich"}}"#;
        assert_eq!(caller.open_control(&host.seal_control(caption).unwrap()).unwrap().trim_end(), caption);
        assert_eq!(caller.open_control(&host.seal_control(&"x".repeat(257)).unwrap()).unwrap().len(), 512);
        // Each end seals its own direction only, and only the host seals text.
        assert_eq!(caller.open_audio(&up).err(), Some(No::Frame));
        assert_eq!(caller.seal_control("{}").err(), Some(No::Input));
        assert_eq!(host.open_control("AAAA").err(), Some(No::Input));
        assert_eq!(caller.open_control("not base64url!").err(), Some(No::Frame));
        // A relay that answers with a hello of its own eph: any tag, also the real host's.
        let mut forged = host_hello;
        forged[22..54].copy_from_slice(&CallHandshake::new(CallRole::Host, Some(root), 3).unwrap().hello[22..]);
        let again = CallHandshake::new(CallRole::Caller, None, 0).unwrap();
        assert_eq!(again.finish(&input(&forged)).err(), Some(No::Tag));
        assert_eq!(again.finish(&input(&host_hello)).err(), Some(No::Finished));
    }
}
