//! #EXP-2123: WHO MAKES A CHAT KEY (`packages/domain-contract/fixtures/install-trust.json`
//! `keyMaker`, its `$comment` the spec; TS `install-trust.ts` `keyMakerStep`,
//! `isKeyMakerRefusal`, `keyMakerWaits`, `strandedRotationEpoch`). Only an
//! install that is JOINED (on its account's chain) creates a tree's first
//! key or rotates one: a key made by an install that waits would be wrapped
//! for itself only and exist nowhere else. Feature-free: `coding` (no eNet
//! node) reads the refusal too; `install_trust` re-exports it.

/// The server's refusal of a key write by an install that waits for approval.
pub const KEY_MAKER_REFUSAL: &str = "An install waiting for approval makes no chat key";
/// Its tRPC code (an HTTP 403).
pub const KEY_MAKER_REFUSAL_CODE: &str = "FORBIDDEN";

/// Whether a server message is [`KEY_MAKER_REFUSAL`] (tRPC may wrap it).
pub fn is_key_maker_refusal(message: &str) -> bool {
    message == KEY_MAKER_REFUSAL || message.contains(KEY_MAKER_REFUSAL)
}

/// `serverCases`: whether the server lists an install as waiting (the New
/// install rule of `installs.waiting`).
pub fn key_maker_waits(chained: bool, asked: bool, account_chain: bool, registration: bool) -> bool {
    !chained && (asked || (account_chain && registration))
}

/// A key write: an existing keyless root's first key, a new root's inline
/// key, a due rotation, a device keying its tree (`tree_key`, a claim).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyWrite {
    FirstKey,
    NewRoot,
    Rotation,
    DeviceKey,
}

impl KeyWrite {
    pub fn parse(text: &str) -> Option<KeyWrite> {
        Some(match text {
            "firstKey" => KeyWrite::FirstKey,
            "newRoot" => KeyWrite::NewRoot,
            "rotation" => KeyWrite::Rotation,
            "deviceKey" => KeyWrite::DeviceKey,
            _ => return None,
        })
    }
}

/// What an install does with a key write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMakerStep {
    /// It makes the key.
    Make,
    /// Nothing is created; the tree's devices are asked (`sessionKeys.requestKey`).
    RequestKey,
    /// The send fails with the no-key refusal; no root is made.
    Refused,
    /// Nothing is created and nothing is remembered.
    Skip,
}

impl KeyMakerStep {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyMakerStep::Make => "make",
            KeyMakerStep::RequestKey => "requestKey",
            KeyMakerStep::Refused => "refused",
            KeyMakerStep::Skip => "skip",
        }
    }
}

/// `makerCases`: `joined` = the pin store's (an older server without
/// `installs.trust` counts as joined); `refused` = the server answered
/// [`KEY_MAKER_REFUSAL`] to an install that took itself for joined.
pub fn key_maker_step(joined: bool, write: KeyWrite, refused: bool) -> KeyMakerStep {
    if joined && !refused {
        return KeyMakerStep::Make;
    }
    match write {
        KeyWrite::FirstKey => KeyMakerStep::RequestKey,
        KeyWrite::NewRoot => KeyMakerStep::Refused,
        KeyWrite::Rotation | KeyWrite::DeviceKey => KeyMakerStep::Skip,
    }
}

/// `strandedCases`: the epoch a holder creates for a root `sessionKeys.due`
/// names (None = nothing to rotate here). An install that waits never
/// rotates; a STRANDED root (no approved reader holds its current epoch) is
/// rotated by a joined reader without the old secret; `left` = the server
/// refused this (root, epoch) before.
pub fn stranded_rotation_epoch(flagged: Option<u32>, current: Option<u32>, held: bool, stranded: bool, joined: bool, left: bool) -> Option<u32> {
    let (flagged, current) = (flagged?, current?);
    if !joined || current != flagged {
        return None;
    }
    if !(held || stranded) || left || current >= i32::MAX as u32 {
        return None;
    }
    Some(current + 1)
}
