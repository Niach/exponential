//! #EXP-1514 (F5, `fixtures/work-screen.json` PLACE, `placeCases`): where
//! ONE launch's computer use happens. Asked at every launch (the first
//! start, every resume, relaunch and reconfigure): nowhere, the person's
//! screen, or an agent space of the session's own, from the device's switch,
//! the session's `computer_use` (and one day its own place), the Work screen
//! pick, the desktop's offer and whether the person's screen can be driven.
//!
//! The rule ([`resolve`], pure): the device allows {space, person} for Auto,
//! {space} for `Agent space only`, {person} for a picked display; the session
//! likewise for its place. A space when both allow one and the offer is not a
//! stable no (an Exponential Desktop that did not answer is a space run whose
//! first call asks again: never the person's screen); else the person's
//! screen when both allow it and it can be driven; else nothing. The offer
//! is asked only where both allow a space, the screen only where both allow
//! it.
//!
//! What the answer does is the launcher's (`attach_computer_use`): a space
//! run gets `epilot::grant_space`, a person run `epilot::grant`, and a launch
//! that is not a space run closes the session's kept space first. The
//! machine is behind [`Machine`] so a test can stand in for ePilot.

/// Where the session's computer use may happen, as the session says (option
/// B of the design: sessions carry no place yet, so every launch is `Auto`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionPlace {
    #[default]
    Auto,
    /// The person's screen only.
    Person,
    /// An agent space only.
    Space,
}

impl SessionPlace {
    /// The wire value; one this build does not know reads as `Auto`.
    pub fn parse(value: &str) -> Self {
        match value {
            "person" => Self::Person,
            "space" => Self::Space,
            _ => Self::Auto,
        }
    }

    fn allows(self) -> Allowed {
        match self {
            Self::Auto => Allowed { space: true, person: true },
            Self::Person => Allowed { space: false, person: true },
            Self::Space => Allowed { space: true, person: false },
        }
    }
}

/// The device's Work screen pick (`launch_defaults.computerUseDisplayId`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// No pick: an agent space when the device offers one, else the screen.
    Auto,
    /// `Agent space only` (the reserved value `space`): never the screen.
    Space,
    /// A display of the person's screen: never a space.
    Display,
}

impl Pick {
    /// The stored pick: absent or blank = Auto, the reserved word = the
    /// agent space only, anything else is a display id.
    pub fn of(stored: Option<&str>) -> Self {
        match stored.map(str::trim) {
            None | Some("") => Self::Auto,
            Some(epilot::work_screen::SPACE_PICK) => Self::Space,
            Some(_) => Self::Display,
        }
    }

    fn allows(self) -> Allowed {
        match self {
            Self::Auto => Allowed { space: true, person: true },
            Self::Space => Allowed { space: true, person: false },
            Self::Display => Allowed { space: false, person: true },
        }
    }
}

/// The desktop's answer to "is there an agent space" ([`epilot::space_offer`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    Offered,
    /// A stable no (not Exponential Desktop, an older one, no user manager,
    /// the driver).
    No,
    /// Exponential Desktop did not answer (yet).
    Unknown,
}

impl Offer {
    pub fn of(offer: &epilot::SpaceOffer) -> Self {
        match offer {
            epilot::SpaceOffer::Offered => Self::Offered,
            epilot::SpaceOffer::No(_) => Self::No,
            epilot::SpaceOffer::Unknown => Self::Unknown,
        }
    }
}

/// Where a launch's computer use happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    None,
    Person,
    Space,
}

/// Where one side lets a run work.
#[derive(Clone, Copy)]
struct Allowed {
    space: bool,
    person: bool,
}

/// What a launch knows without asking the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inputs {
    /// The device's Computer use switch: the gate.
    pub switch: bool,
    /// The session's `computer_use`: `false` = it opted out.
    pub computer_use: bool,
    pub session: SessionPlace,
    pub pick: Pick,
}

/// The launcher's answer (pure, `placeCases`). `offer` is asked only where
/// both sides allow a space, `person` (can the person's screen be driven)
/// only where both allow the screen and no space was the answer.
pub fn resolve(inputs: &Inputs, offer: impl FnOnce() -> Offer, person: impl FnOnce() -> bool) -> Place {
    if !inputs.switch || !inputs.computer_use {
        return Place::None;
    }
    let (device, session) = (inputs.pick.allows(), inputs.session.allows());
    if device.space && session.space && offer() != Offer::No {
        return Place::Space;
    }
    if device.person && session.person && person() {
        return Place::Person;
    }
    Place::None
}

/// What a launch got ([`crate::launcher`]'s `attach_computer_use`), for its
/// prompt ([`crate::skill::ComputerUse`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placed {
    /// No computer use. `Some(why)` = the switch is on, the screen was the
    /// place, and it cannot be driven (row 5): the run's prompt gets one
    /// line saying why, so the agent can tell the person.
    None(Option<String>),
    /// The person's screen, today's grant.
    Person,
    /// An agent space of the session's own.
    Space,
}

/// What a launch asks this machine about computer use: ePilot in a host
/// ([`Epilot`]), a stand-in in a test ([`with_machine`]).
pub trait Machine {
    fn space_offer(&self) -> Offer;
    /// Whether the person's screen can be driven here; `Err` says why not.
    fn screen(&self) -> Result<(), String>;
    /// This host keeps an agent space for the session.
    fn kept(&self, session_id: &str) -> bool;
    fn close_space(&self, session_id: &str);
    fn grant(&self, session_id: &str, label: &str) -> Result<epilot::Grant, String>;
    fn grant_space(&self, session_id: &str, label: &str, had_space: bool) -> Result<epilot::Grant, String>;
}

/// The host's own machine.
pub struct Epilot;

impl Machine for Epilot {
    /// #EXP-2065: asked now when the cache holds no fresh real answer.
    fn space_offer(&self) -> Offer {
        Offer::of(&epilot::space_offer_now())
    }

    /// The grant's own gate: a machine with nothing to drive (no display, a
    /// Wayland session other than eDE, eDE without its socket). A missing
    /// OS permission still gets the grant, as before: its calls say which.
    fn screen(&self) -> Result<(), String> {
        match epilot::readiness() {
            epilot::Readiness::Unsupported(reason) => Err(reason),
            epilot::Readiness::Ready | epilot::Readiness::MissingPermission(_) => Ok(()),
        }
    }

    fn kept(&self, session_id: &str) -> bool {
        epilot::kept_spaces().iter().any(|kept| kept == session_id)
    }

    fn close_space(&self, session_id: &str) {
        epilot::close_space(session_id);
    }

    fn grant(&self, session_id: &str, label: &str) -> Result<epilot::Grant, String> {
        epilot::grant(session_id, label)
    }

    fn grant_space(&self, session_id: &str, label: &str, had_space: bool) -> Result<epilot::Grant, String> {
        epilot::grant_space(session_id, label, had_space)
    }
}

#[cfg(test)]
thread_local! {
    static FAKE: std::cell::RefCell<Option<std::rc::Rc<dyn Machine>>> = const { std::cell::RefCell::new(None) };
}

/// `run` with the machine every launch on this thread asks ([`Epilot`]
/// outside a test).
pub fn machine<R>(run: impl FnOnce(&dyn Machine) -> R) -> R {
    #[cfg(test)]
    if let Some(fake) = FAKE.with(|fake| fake.borrow().clone()) {
        return run(&*fake);
    }
    run(&Epilot)
}

/// A test's stand-in for ePilot while `run` runs on this thread.
#[cfg(test)]
pub(crate) fn with_machine<R>(fake: std::rc::Rc<dyn Machine>, run: impl FnOnce() -> R) -> R {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAKE.with(|slot| slot.borrow_mut().take());
        }
    }
    FAKE.with(|slot| *slot.borrow_mut() = Some(fake));
    let _reset = Reset;
    run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/work-screen.json")).unwrap()
    }

    fn offer(value: &str) -> Offer {
        match value {
            "offered" => Offer::Offered,
            "no" => Offer::No,
            "unknown" => Offer::Unknown,
            other => panic!("an offer the fixture does not define: {other}"),
        }
    }

    fn pick(value: &str) -> Pick {
        match value {
            "auto" => Pick::Auto,
            "space" => Pick::Space,
            "display" => Pick::Display,
            other => panic!("a pick the fixture does not define: {other}"),
        }
    }

    fn place(value: &str) -> Place {
        match value {
            "none" => Place::None,
            "person" => Place::Person,
            "space" => Place::Space,
            other => panic!("a place the fixture does not define: {other}"),
        }
    }

    /// #EXP-1514: every `placeCases` row, ×4 with the TS reference. The
    /// offer is asked only where both sides allow a space; a display pick
    /// never asks it (the desktop's silence is not the screen's business).
    #[test]
    fn the_place_replays_the_fixture() {
        let cases = fixture()["placeCases"].as_array().unwrap().clone();
        assert!(cases.len() >= 21, "{} cases", cases.len());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let inputs = Inputs {
                switch: case["switch"].as_bool().unwrap(),
                computer_use: case["session"]["computerUse"].as_bool().unwrap(),
                session: SessionPlace::parse(case["session"]["place"].as_str().unwrap_or("auto")),
                pick: pick(case["pick"].as_str().unwrap()),
            };
            let asked = Cell::new(false);
            let got = resolve(
                &inputs,
                || {
                    asked.set(true);
                    offer(case["offer"].as_str().unwrap())
                },
                || case["person"].as_bool().unwrap(),
            );
            assert_eq!(got, place(case["expect"].as_str().unwrap()), "{name}");
            let space_allowed = inputs.switch && inputs.computer_use && inputs.pick != Pick::Display && inputs.session != SessionPlace::Person;
            assert_eq!(asked.get(), space_allowed, "{name}: the offer is asked only where both allow a space");
        }
    }

    /// `unknown` is never the person's screen, whatever can be driven.
    #[test]
    fn a_desktop_that_did_not_answer_is_never_the_screen() {
        for pick in [Pick::Auto, Pick::Space] {
            let inputs = Inputs { switch: true, computer_use: true, session: SessionPlace::Auto, pick };
            assert_eq!(resolve(&inputs, || Offer::Unknown, || panic!("the screen is not asked")), Place::Space);
        }
    }

    /// #EXP-1514: an eDE that answers without `space-readiness` (every one
    /// before the flip) is an older desktop, and Auto resolves to the
    /// person's screen, as before spaces; no answer (#EXP-2065) is a space.
    #[test]
    fn an_older_desktops_answer_is_the_screen_as_before() {
        let inputs = Inputs { switch: true, computer_use: true, session: SessionPlace::Auto, pick: Pick::Auto };
        let older = Offer::of(&epilot::SpaceOffer::No(epilot::SpaceBlock::OlderDesktop));
        assert_eq!(resolve(&inputs, || older, || true), Place::Person);
        assert_eq!(resolve(&inputs, || Offer::of(&epilot::SpaceOffer::Unknown), || true), Place::Space);
    }

    #[test]
    fn the_stored_pick_reads_auto_space_or_a_display() {
        assert_eq!(Pick::of(None), Pick::Auto);
        assert_eq!(Pick::of(Some(" ")), Pick::Auto);
        assert_eq!(Pick::of(Some("space")), Pick::Space);
        assert_eq!(Pick::of(Some(fixture()["space"]["value"].as_str().unwrap())), Pick::Space);
        for display in ["output:DP-1", "edid:abc@HDMI-A-1", "cg:1", "\\\\.\\DISPLAY1", "spaces"] {
            assert_eq!(Pick::of(Some(display)), Pick::Display, "{display}");
        }
        assert_eq!(SessionPlace::parse("later"), SessionPlace::Auto);
    }
}
