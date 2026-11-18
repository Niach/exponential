//! #EXP-1941: the call seal (`enet_core::call_seal`; packages/etalk/fixtures/
//! voice-call.json `seal` is the spec) for the phone callers: the two objects
//! and ONE stateless self-test. No constructor takes an eph secret, so a
//! phone cannot replay a vector itself: `call_seal_selftest` replays
//! `seal.vectors` and `seal.refused` inside Rust and names what failed.
//! `finish` works once, a failed one too; a caller that HAS an
//! `EnetCallSeal` has the host's proof, so "seal when it exists" is the whole
//! gate of a capture path. One `EnetCallSeal` is safe on two threads, one
//! sealing and one opening. A refusal is `EnetError.Crypto` whose reason is
//! `call seal: <reason>` (the fixture's `reason`).

use std::sync::Arc;

use enet_core::call_seal as core;

use crate::EnetError;

fn refused(refusal: core::CallSealRefusal) -> EnetError {
    EnetError::Crypto { reason: refusal.to_string() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EnetCallRole {
    Caller,
    Host,
}

/// The sizes a sealer pads to (`seal.audioPadStep`, `controlPadBytes`, `maxPlainBytes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct EnetCallSealPad {
    pub audio_up: u32,
    pub audio_down: u32,
    pub control_pad: u32,
    pub max_plain: u32,
}

/// The fixture's sizes (the self-test holds them against `seal`).
#[uniffi::export]
pub fn call_seal_pad() -> EnetCallSealPad {
    let pad = core::CallSealPad::default();
    EnetCallSealPad { audio_up: pad.audio_up as u32, audio_down: pad.audio_down as u32, control_pad: pad.control_pad as u32, max_plain: pad.max_plain as u32 }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetCallFinish {
    /// The tree SECRET (32 bytes) of the HOST hello's root and epoch.
    pub secret: Vec<u8>,
    pub call_id: String,
    pub agent_id: String,
    /// The host's steer device id.
    pub device_id: String,
    /// The other end's hello: 70 bytes for a caller, 54 for a host.
    pub peer_hello: Vec<u8>,
    /// CALLER ONLY, required there: the root and the epoch its own
    /// `sendTarget` answer names NOW. A host hello that names others is refused.
    pub root_id: Option<String>,
    pub epoch: u32,
    pub pad: EnetCallSealPad,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EnetOpenedAudio {
    pub seq: u16,
    /// Bit 0 on `up` = the microphone is muted.
    pub flags: u8,
    /// Empty for a frame with `len` 0: nothing of it reaches a decoder.
    pub packet: Vec<u8>,
    /// Frames missing before this packet (a `len` 0 frame counts as one):
    /// never join audio across a gap.
    pub missing: u32,
}

/// One end's half of the exchange: it holds the eph secret until `finish`.
#[derive(uniffi::Object)]
pub struct EnetCallHandshake(core::CallHandshake);

#[uniffi::export]
impl EnetCallHandshake {
    /// A fresh eph key per socket. `root_id` None = a caller that knows no root yet.
    #[uniffi::constructor]
    pub fn new(role: EnetCallRole, root_id: Option<String>, epoch: u32) -> Result<Arc<Self>, EnetError> {
        let role = if role == EnetCallRole::Caller { core::CallRole::Caller } else { core::CallRole::Host };
        Ok(Arc::new(Self(core::CallHandshake::new(role, root_id.as_deref(), epoch).map_err(refused)?)))
    }

    /// The CALLER's hello (54 bytes): made once, resent unchanged.
    pub fn hello(&self) -> Result<Vec<u8>, EnetError> {
        Ok(self.0.hello().map_err(refused)?.to_vec())
    }

    /// The call's sealer. WORKS ONCE, whatever it answers.
    pub fn finish(&self, input: EnetCallFinish) -> Result<Arc<EnetCallSeal>, EnetError> {
        let pad = core::CallSealPad {
            audio_up: input.pad.audio_up as usize,
            audio_down: input.pad.audio_down as usize,
            control_pad: input.pad.control_pad as usize,
            max_plain: input.pad.max_plain as usize,
        };
        let finish = core::CallFinish {
            secret: &input.secret,
            call_id: &input.call_id,
            agent_id: &input.agent_id,
            device_id: &input.device_id,
            peer_hello: &input.peer_hello,
            root_id: input.root_id.as_deref(),
            epoch: input.epoch,
            pad,
        };
        Ok(Arc::new(EnetCallSeal(self.0.finish(&finish).map_err(refused)?)))
    }
}

/// The sealed call of one end; it owns its counters.
#[derive(uniffi::Object)]
pub struct EnetCallSeal(core::CallSeal);

#[uniffi::export]
impl EnetCallSeal {
    /// HOST ONLY: its hello with the tag (70 bytes), sent once.
    pub fn host_hello(&self) -> Option<Vec<u8>> {
        self.0.host_hello().map(|hello| hello.to_vec())
    }

    /// One audio frame, always one size. A packet above the step is refused
    /// (`too_large`): seal an empty packet in its place.
    pub fn seal_audio(&self, seq: u16, flags: u8, packet: Vec<u8>) -> Result<Vec<u8>, EnetError> {
        self.0.seal_audio(seq, flags, &packet).map_err(refused)
    }

    pub fn open_audio(&self, frame: Vec<u8>) -> Result<EnetOpenedAudio, EnetError> {
        let core::OpenedAudio { seq, flags, packet, missing } = self.0.open_audio(&frame).map_err(refused)?;
        Ok(EnetOpenedAudio { seq, flags, packet, missing })
    }

    /// HOST ONLY: the `data` of `call_event {t: "sealed"}`.
    pub fn seal_control(&self, text: String) -> Result<String, EnetError> {
        self.0.seal_control(&text).map_err(refused)
    }

    /// CALLER ONLY: the control frame's JSON text, trailing spaces included.
    pub fn open_control(&self, data: String) -> Result<String, EnetError> {
        self.0.open_control(&data).map_err(refused)
    }
}

/// Replays `seal` of voice-call.json (the whole fixture's JSON) inside Rust;
/// an error's reason names every vector and refusal that did not hold.
#[uniffi::export]
pub fn call_seal_selftest(fixture_json: String) -> Result<(), EnetError> {
    core::selftest(&fixture_json).map_err(|reason| EnetError::Crypto { reason })
}
