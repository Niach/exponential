//! EXP-1330 — a `/btw` aside run's one-shot end (EXP-1426 part 4d, out of
//! `engine::aside`, which re-exports it beside the tool posture): after its
//! FIRST reply the aside ends (the reply itself posts like any turn's),
//! unless a person spoke in it meanwhile: then it is an ordinary session.

use std::sync::atomic::{AtomicBool, Ordering};

/// The aside's one-shot state on its [`super::host::SessionCtx`].
#[derive(Debug, Default)]
pub struct AsideState {
    /// A turn has started (the seed question went to the agent): the idle
    /// edges before it (the handshake) are no answer.
    turn_started: AtomicBool,
    /// A person's message was handed to the run.
    person_spoke: AtomicBool,
    /// The first turn ended: decided once, never again.
    settled: AtomicBool,
}

/// What the idle edge decided.
#[derive(Debug, PartialEq, Eq)]
pub enum AsideEnd {
    /// Not an answer edge (no turn yet, already settled).
    Nothing,
    /// A person spoke: the run carries on as an ordinary session.
    KeepGoing,
    /// End the run.
    End,
}

impl AsideState {
    pub fn turn_started(&self) {
        self.turn_started.store(true, Ordering::SeqCst);
    }

    pub fn person_spoke(&self) {
        self.person_spoke.store(true, Ordering::SeqCst);
    }

    /// The idle edge: the first turn's end settles the aside.
    pub fn turn_ended(&self) -> AsideEnd {
        if !self.turn_started.load(Ordering::SeqCst) || self.settled.swap(true, Ordering::SeqCst) {
            return AsideEnd::Nothing;
        }
        if self.person_spoke.load(Ordering::SeqCst) {
            return AsideEnd::KeepGoing;
        }
        AsideEnd::End
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_answer_ends_the_aside() {
        let aside = AsideState::default();
        assert_eq!(aside.turn_ended(), AsideEnd::Nothing, "the handshake's idle edge");
        aside.turn_started();
        assert_eq!(aside.turn_ended(), AsideEnd::End);
        assert_eq!(aside.turn_ended(), AsideEnd::Nothing, "settled once");
    }

    #[test]
    fn a_person_reply_keeps_the_aside_running() {
        let aside = AsideState::default();
        aside.turn_started();
        aside.person_spoke();
        assert_eq!(aside.turn_ended(), AsideEnd::KeepGoing);
    }
}
