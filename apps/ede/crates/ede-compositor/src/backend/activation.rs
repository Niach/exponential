//! A compositor started while its session is not the seat's active one
//! waits here, before it opens a GPU (#EDE-218): logind hands an inactive
//! session its DRM device without DRM master, so every modeset is
//! "Permission denied".
//!
//! Who has the displays is asked of logind itself (`loginctl`), never of the
//! seat library (#EDE-219). libseat's logind backend learns that its session
//! became active only from a DRM device's `ResumeDevice` signal before 0.9
//! (Ubuntu 24.04 and Mint 22 ship 0.8.0): its handler for logind's `Active`
//! property returns before it reads anything (`if (!is_session ||
//! !is_seat) return 0;`). Before a GPU is opened there is no such device,
//! so a wait on the library never ended there: a black screen at every
//! login from lightdm, which starts a Wayland session before it switches to
//! its VT (GDM switches first).
//!
//! So the wait polls logind, and opens the seat anew when logind says the
//! session is active: a fresh libseat session reads `Active` at its open.
//! While another session has the displays (the login screen after logind
//! restarted, another VT) the wait has no end: that session is what the
//! person sees. While nobody has them (the display manager let go of its
//! greeter and has not switched yet, a text console), logind is asked for
//! them every [`ASK_EVERY`] (`loginctl activate`, which a session may ask
//! for itself; libseat 0.8 asks once at its open, 0.9 never), and after
//! [`GIVE_UP`] the start fails with the reason, which ends at the login
//! screen instead of a black one.

use std::time::{Duration, Instant};

/// How often logind is asked.
pub const POLL: Duration = Duration::from_millis(250);
/// While nobody has the displays: how often logind is asked to activate
/// this session.
pub const ASK_EVERY: Duration = Duration::from_secs(1);
/// While nobody has the displays: how long until the start fails.
pub const GIVE_UP: Duration = Duration::from_secs(10);

/// Who has the seat's displays, as logind tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// This session is the active one.
    Ours,
    /// Another session that is alive: a login screen, another VT.
    Other,
    /// No session, one that is closing, or logind does not answer.
    Nobody,
}

/// The value of `key` in `loginctl show-…` output (`Key=value` lines).
fn property<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
}

/// Who has `seat`'s displays. `loginctl` runs `loginctl` with these
/// arguments (`None` = it failed: no logind, an unknown session).
/// `session` is ours: `$XDG_SESSION_ID`, else `auto` (logind picks the
/// caller's).
pub fn holder(seat: &str, session: &str, mut loginctl: impl FnMut(&[&str]) -> Option<String>) -> Holder {
    let Some(ours) = loginctl(&["show-session", session, "-p", "Id", "-p", "Active"]) else { return Holder::Nobody };
    if property(&ours, "Active") == Some("yes") {
        return Holder::Ours;
    }
    let active = loginctl(&["show-seat", seat, "-p", "ActiveSession"]);
    let Some(active) = active.as_deref().and_then(|o| property(o, "ActiveSession")).filter(|id| !id.is_empty()) else { return Holder::Nobody };
    if Some(active) == property(&ours, "Id") {
        // Logind is switching to us right now.
        return Holder::Nobody;
    }
    // A session that logged out keeps its VT until its last process is
    // gone ("closing"): nobody looks at it.
    match loginctl(&["show-session", active, "-p", "State"]).as_deref().and_then(|o| property(o, "State")) {
        Some("active") | Some("online") => Holder::Other,
        _ => Holder::Nobody,
    }
}

/// `loginctl` for [`holder`].
#[cfg_attr(not(feature = "udev"), allow(dead_code))]
pub fn loginctl(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("loginctl").args(args).stderr(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `loginctl activate` for `session` ([`holder`]'s): logind switches to its
/// VT. Allowed without a password from inside the session itself.
#[cfg_attr(not(feature = "udev"), allow(dead_code))]
pub fn activate(session: &str) {
    let args: &[&str] = if session == "auto" { &["activate"] } else { &["activate", session] };
    if loginctl(args).is_none() {
        tracing::warn!("logind did not activate this session (loginctl activate failed)");
    }
}

/// What the wait does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Logind says the session is active: open the seat anew and go on.
    Go,
    /// Sleep [`POLL`] and ask again.
    Wait,
    /// Nobody has the displays: ask logind for them.
    Ask,
    /// Nobody took the displays in [`GIVE_UP`]: the start fails.
    GiveUp,
}

/// The wait's memory: since when nobody has had the displays, and when the
/// seat was last asked for.
#[derive(Debug, Default)]
pub struct Wait {
    nobody_since: Option<Instant>,
    asked: Option<Instant>,
}

impl Wait {
    pub fn step(&mut self, holder: Holder, now: Instant) -> Step {
        match holder {
            Holder::Ours => Step::Go,
            Holder::Other => {
                *self = Self::default();
                Step::Wait
            }
            Holder::Nobody => {
                let since = *self.nobody_since.get_or_insert(now);
                if now.duration_since(since) >= GIVE_UP {
                    Step::GiveUp
                } else if now.duration_since(self.asked.unwrap_or(since)) >= ASK_EVERY {
                    self.asked = Some(now);
                    Step::Ask
                } else {
                    Step::Wait
                }
            }
        }
    }
}

/// Why a start that gave up failed, for the recovery screen and the journal.
pub fn gave_up_message(seat: &str) -> String {
    format!(
        "this session never got {seat}'s displays: nobody had them for {} s after the start and logind did not make this session the active one (`loginctl session-status` names its state)",
        GIVE_UP.as_secs()
    )
}

/// Waits until `seat` (an open seat session that said it is not active) is
/// one that is. `reopen` closes the session it is given and opens a new
/// one; `holder` asks logind who has the displays and `ask` asks it for
/// them; `sleep` waits and `now` tells the time. The active session, or why
/// the start fails.
pub fn wait_for_seat<S>(
    mut seat: S,
    is_active: impl Fn(&S) -> bool,
    mut reopen: impl FnMut(S) -> Result<S, String>,
    mut holder: impl FnMut() -> Holder,
    mut ask: impl FnMut(),
    mut sleep: impl FnMut(Duration),
    mut now: impl FnMut() -> Instant,
    gave_up: impl FnOnce() -> String,
) -> Result<S, String> {
    let mut wait = Wait::default();
    let mut last = None;
    while !is_active(&seat) {
        let holder = holder();
        if last != Some(holder) {
            match holder {
                Holder::Ours => tracing::info!("logind made this session the active one"),
                Holder::Other => tracing::info!("another session has the displays; waiting for it, however long"),
                Holder::Nobody => tracing::warn!("no session has the displays; asking logind for them every {ASK_EVERY:?}, for {GIVE_UP:?}"),
            }
            last = Some(holder);
        }
        match wait.step(holder, now()) {
            Step::Go => seat = reopen(seat)?,
            Step::Ask => ask(),
            Step::Wait => {}
            Step::GiveUp => return Err(gave_up()),
        }
        if !is_active(&seat) {
            sleep(POLL);
        }
    }
    Ok(seat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// `loginctl` over canned sessions `(id, active, state)` on seat0,
    /// `active_on_seat` being the seat's `ActiveSession`.
    fn logind<'a>(sessions: &'a [(&'a str, bool, &'a str)], active_on_seat: &'a str) -> impl FnMut(&[&str]) -> Option<String> + 'a {
        move |args| match args {
            ["show-session", id, rest @ ..] => {
                let (id, active, state) = sessions.iter().find(|s| s.0 == *id)?;
                Some(if rest.contains(&"State") { format!("State={state}\n") } else { format!("Id={id}\nActive={}\n", if *active { "yes" } else { "no" }) })
            }
            ["show-seat", "seat0", ..] => Some(format!("ActiveSession={active_on_seat}\n")),
            _ => None,
        }
    }

    #[test]
    fn logind_says_who_has_the_displays() {
        // Every normal login: ours.
        assert_eq!(holder("seat0", "c6", logind(&[("c6", true, "active")], "c6")), Holder::Ours);
        // #EDE-218: the login screen after logind restarted.
        assert_eq!(holder("seat0", "c6", logind(&[("c6", false, "online"), ("c7", true, "active")], "c7")), Holder::Other);
        // #EDE-219, mint at 12:31:01: lightdm's greeter session is gone,
        // the VT still belongs to the session that logged out before it.
        assert_eq!(holder("seat0", "c6", logind(&[("c6", false, "online"), ("c2", false, "closing")], "c2")), Holder::Nobody);
        // A text console: the seat has no active session at all.
        assert_eq!(holder("seat0", "c6", logind(&[("c6", false, "online")], "")), Holder::Nobody);
        // Logind names us on the seat before the session says so itself.
        assert_eq!(holder("seat0", "c6", logind(&[("c6", false, "online")], "c6")), Holder::Nobody);
        // No logind, or a session it does not know.
        assert_eq!(holder("seat0", "auto", |_| None), Holder::Nobody);
    }

    /// A seat library like libseat on logind: what a session says about
    /// itself is what logind said when it was opened, and never changes
    /// while no DRM device is open (#EDE-219).
    struct Rig {
        clock: Cell<Duration>,
        start: Instant,
        /// When logind makes the session active, if ever.
        active_at: Option<Duration>,
        /// Another live session has the displays until then.
        other_until: Duration,
        opens: Cell<usize>,
        asks: Cell<usize>,
        holders: RefCell<Vec<Holder>>,
    }

    impl Rig {
        fn new(active_at: Option<Duration>, other_until: Duration) -> Self {
            Self { clock: Cell::new(Duration::ZERO), start: Instant::now(), active_at, other_until, opens: Cell::new(1), asks: Cell::new(0), holders: RefCell::default() }
        }
        fn logind_active(&self) -> bool {
            self.active_at.is_some_and(|at| self.clock.get() >= at)
        }
        /// The wait, from a session opened inactive; the session it ends
        /// with (`true` = active) or the failure.
        fn wait(&self) -> Result<bool, String> {
            wait_for_seat(
                false,
                |active| *active,
                |_| {
                    self.opens.set(self.opens.get() + 1);
                    Ok(self.logind_active())
                },
                || {
                    let holder = if self.logind_active() {
                        Holder::Ours
                    } else if self.clock.get() < self.other_until {
                        Holder::Other
                    } else {
                        Holder::Nobody
                    };
                    self.holders.borrow_mut().push(holder);
                    holder
                },
                || self.asks.set(self.asks.get() + 1),
                |d| self.clock.set(self.clock.get() + d),
                || self.start + self.clock.get(),
                || gave_up_message("seat0"),
            )
        }
    }

    #[test]
    fn a_session_lightdm_activates_after_the_start_gets_its_displays() {
        // #EDE-219: lightdm switches to the session's VT 400 ms after the
        // compositor opened its seat. The session opened then never says
        // "active"; the one opened when logind does, does.
        let rig = Rig::new(Some(Duration::from_millis(400)), Duration::ZERO);
        assert_eq!(rig.wait(), Ok(true));
        assert_eq!(rig.clock.get(), Duration::from_millis(500), "found at the next poll");
        assert_eq!((rig.opens.get(), rig.asks.get()), (2, 0), "the seat is opened anew once, when logind says active");
    }

    #[test]
    fn a_login_screen_with_the_displays_is_waited_for_however_long() {
        // #EDE-218: the greeter has the displays for an hour; then the
        // person signs in and the display manager activates the session.
        let hour = Duration::from_secs(3600);
        let rig = Rig::new(Some(hour), hour);
        assert_eq!(rig.wait(), Ok(true));
        assert_eq!((rig.opens.get(), rig.asks.get()), (2, 0), "the seat is not asked for while another session shows");
    }

    #[test]
    fn a_seat_nobody_has_is_asked_for_and_then_given_up() {
        // Nobody ever activates the session: logind is asked every second,
        // and the start fails after ten, with a reason, instead of a black
        // screen for good.
        let rig = Rig::new(None, Duration::ZERO);
        let failed = rig.wait().unwrap_err();
        assert!(failed.contains("never got seat0's displays"), "{failed}");
        assert_eq!(rig.clock.get(), GIVE_UP);
        assert_eq!((rig.opens.get(), rig.asks.get()), (1, 9), "asked at 1 s … 9 s");
        // The ten seconds count from when the displays were let go.
        let rig = Rig::new(None, Duration::from_secs(60));
        assert!(rig.wait().is_err());
        assert_eq!(rig.clock.get(), Duration::from_secs(60) + GIVE_UP);
    }

    #[test]
    fn asking_for_the_seat_is_what_gets_it() {
        // Logind activates the session only because it was asked (libseat
        // 0.9 never asks; 0.8's one `Activate` raced the greeter's X server
        // letting go): active from the second ask on.
        let rig = Rig::new(None, Duration::ZERO);
        let asked = Cell::new(0);
        let seat = wait_for_seat(
            false,
            |active| *active,
            |_| Ok(true),
            || if asked.get() == 2 { Holder::Ours } else { Holder::Nobody },
            || asked.set(asked.get() + 1),
            |d| rig.clock.set(rig.clock.get() + d),
            || rig.start + rig.clock.get(),
            || unreachable!("the seat came"),
        );
        assert_eq!((seat, rig.clock.get()), (Ok(true), Duration::from_millis(2250)));
    }

    #[test]
    fn a_seat_that_cannot_be_opened_anew_fails_the_start() {
        let seat = wait_for_seat(false, |a| *a, |_| Err("no seat session: EBUSY".into()), || Holder::Ours, || {}, |_| {}, Instant::now, || unreachable!());
        assert_eq!(seat, Err("no seat session: EBUSY".to_string()));
    }
}
