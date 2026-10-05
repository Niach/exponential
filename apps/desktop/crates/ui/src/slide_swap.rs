//! EXP-1192: the ONE slide-swap state machine — the shell's left column
//! (rail ⇄ settings nav, [`crate::shell::LeftOccupant`]) and the content
//! card's second sidebar (`Option<SecondSidebar>`) both animate through it.
//!
//! EXP-456/EXP-851: the upstream `Sidebar` recipe (`gpui_component::sidebar`'s
//! animation state) as plain fields: WHICH occupants are coming and going,
//! whether both children stay mounted, and an epoch guarding the unmount
//! timer against retargets. The widths are not state — every width is a pure
//! function of the occupant the host reads per frame.

/// The swap of a slot between two occupants of type `T`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SwapAnim<T: Copy + PartialEq> {
    /// The occupant sliding OUT (only meaningful while `swapping`).
    pub from: T,
    /// Target occupant.
    pub to: T,
    /// Both children stay mounted while the swap transition runs.
    pub swapping: bool,
    /// Guards the unmount timer against retargets (upstream `hide_request`).
    pub epoch: u64,
}

impl<T: Copy + PartialEq> SwapAnim<T> {
    pub(crate) fn new(occupant: T) -> Self {
        Self {
            from: occupant,
            to: occupant,
            swapping: false,
            epoch: 0,
        }
    }

    /// Sync with the rendered state. Returns `Some(epoch)` when a swap
    /// STARTED and the caller must spawn the settle timer.
    pub(crate) fn retarget(&mut self, occupant: T) -> Option<u64> {
        if self.to == occupant {
            // No change (this runs on every render).
            return None;
        }
        // Mid-flight reversal restarts the slide from the occupant that is
        // live right now (upstream has the same limitation) — acceptable over
        // one `STANDARD` motion.
        self.from = self.to;
        self.to = occupant;
        self.swapping = true;
        self.epoch += 1;
        Some(self.epoch)
    }

    /// Timer callback. True = the swap this epoch belongs to just settled.
    pub(crate) fn finish(&mut self, epoch: u64) -> bool {
        if self.swapping && self.epoch == epoch {
            self.swapping = false;
            self.from = self.to;
            true
        } else {
            false
        }
    }
}

/// EXP-456/EXP-862: an animated element's id. It carries the EPOCH, so a
/// retargeted swap gets a new id and restarts cleanly (upstream
/// `sidebar_animation_id`, which encoded the from/to widths).
pub(crate) fn anim_id(name: &'static str, epoch: u64) -> gpui::ElementId {
    gpui::ElementId::NamedInteger(name.into(), epoch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Slot {
        A,
        B,
        C,
    }

    #[test]
    fn retarget_starts_a_swap_and_bumps_the_epoch() {
        let mut anim = SwapAnim::new(Slot::A);
        assert_eq!(anim.retarget(Slot::B), Some(1));
        assert!(anim.swapping);
        assert_eq!(anim.from, Slot::A);
        assert_eq!(anim.to, Slot::B);
    }

    /// The same machine in both directions, and (EXP-862) a re-render with
    /// the same occupant is a no-op, which is what keeps the slide off every
    /// frame.
    #[test]
    fn swaps_in_both_directions() {
        let mut anim = SwapAnim::new(Slot::A);
        assert_eq!(anim.retarget(Slot::B), Some(1));
        assert!(anim.finish(1));
        assert!(!anim.swapping);
        assert_eq!(anim.from, Slot::B);
        // … and back out.
        assert_eq!(anim.retarget(Slot::A), Some(2));
        assert!(anim.swapping);
        assert_eq!(anim.from, Slot::B);
        assert_eq!(anim.to, Slot::A);
        assert_eq!(anim.retarget(Slot::A), None);
    }

    /// EXP-1192: the card's second sidebar swaps an `Option` — opening,
    /// switching between two sidebars and closing are all plain swaps.
    #[test]
    fn an_optional_occupant_swaps_like_any_other() {
        let mut anim = SwapAnim::new(None);
        let epoch = anim.retarget(Some(Slot::A)).unwrap();
        assert!(anim.finish(epoch));
        let epoch = anim.retarget(Some(Slot::C)).unwrap();
        assert_eq!((anim.from, anim.to), (Some(Slot::A), Some(Slot::C)));
        assert!(anim.finish(epoch));
        assert_eq!(anim.retarget(Some(Slot::C)), None);
        assert!(anim.retarget(None).is_some());
    }

    #[test]
    fn finish_ignores_stale_epochs() {
        let mut anim = SwapAnim::new(Slot::A);
        let first = anim.retarget(Slot::B).unwrap();
        // Mid-flight reversal: the new swap replaces the old one.
        let second = anim.retarget(Slot::A).unwrap();
        assert_ne!(first, second);
        // The superseded timer fires anyway (cancellation raced) — no-op.
        assert!(!anim.finish(first));
        assert!(anim.swapping);
        // The live one settles the swap.
        assert!(anim.finish(second));
        assert!(!anim.swapping);
        assert_eq!(anim.from, anim.to);
    }

    /// Upstream sidebar rule: a retarget restarts the transition (the
    /// animation id carries the epoch, so the visual jump is bounded by one
    /// swap). EXP-851: a mid-flight reversal also swaps the OUTGOING occupant
    /// back, or the strip would keep sliding the wrong child out.
    #[test]
    fn midflight_reversal_restarts_from_the_live_occupant() {
        let mut anim = SwapAnim::new(Slot::A);
        let first = anim.retarget(Slot::B).unwrap();
        let second = anim.retarget(Slot::A).unwrap();
        assert_eq!(anim.from, Slot::B);
        assert_eq!(anim.to, Slot::A);
        assert_ne!(anim_id("slide", first), anim_id("slide", second));
    }
}
