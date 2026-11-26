//! Exponential patch (#EDE-227): an xdg-activation token handed in from
//! outside the process. Exponential Desktop's shell mints a launch token
//! for a press and sends it with the link over the app's session bus; the
//! next `activate` of a window (the one the link lands in) passes that
//! token to `xdg_activation_v1.activate` instead of asking for one of its
//! own, which the compositor would not honour without a recent input
//! serial of this client.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// A handed token older than this is dropped (a compositor's launch token
/// lives about as long).
const HANDED_TTL: Duration = Duration::from_secs(30);

static HANDED: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// The next window `activate` uses `token` (one-shot; a newer one replaces it).
pub fn hand_activation_token(token: String) {
    if let Ok(mut handed) = HANDED.lock() {
        *handed = Some((token, Instant::now()));
    }
}

/// The handed token while it is fresh, taken.
#[allow(dead_code)]
pub(crate) fn take_handed_activation_token() -> Option<String> {
    let (token, at) = HANDED.lock().ok()?.take()?;
    (at.elapsed() < HANDED_TTL).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handed_token_is_taken_once() {
        hand_activation_token("a".into());
        hand_activation_token("b".into());
        assert_eq!(take_handed_activation_token().as_deref(), Some("b"));
        assert_eq!(take_handed_activation_token(), None);
    }
}
