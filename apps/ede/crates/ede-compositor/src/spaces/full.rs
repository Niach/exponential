//! #EXP-1514 (D 10 = B): a full desktop gives way. A space lives as long as
//! its session, and a person's chat never ends by itself, so spaces add up.
//! When a NEW space does not fit (the count, or the memory under the slice's
//! `MemoryHigh`), the space unused longest that is neither in use nor busy
//! closes and the new one is made; for memory as many as it takes, planned
//! before one closes. In use = driven or captured by its own grant within
//! [`super::Spaces::in_use_for`] (a space never used counts from its
//! creation); busy = its keeper said `space_busy` within
//! [`super::Spaces::busy_for`] (its session's turn runs). Only when no such
//! space is left is the answer `limit`, and nothing closed for it.

use std::time::{Duration, Instant};

use ede_protocol::{ErrorCode, Failure, SessionId};

use crate::{desk::DeskId, state::Ede};

/// How long a use keeps a space from giving way.
pub const IN_USE_FOR: Duration = Duration::from_secs(10 * 60);
/// How long one `space_busy` does.
pub const BUSY_FOR: Duration = Duration::from_secs(ede_protocol::SPACE_BUSY_SECS);

/// One space as the rule reads it.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub desk: DeskId,
    /// Its last use, else its creation.
    pub used: Instant,
    pub busy_until: Option<Instant>,
}

/// The spaces that may give way at `now`, the one unused longest first.
pub fn giving_way(spaces: &[Candidate], now: Instant, in_use_for: Duration) -> Vec<DeskId> {
    let mut free: Vec<&Candidate> = spaces
        .iter()
        .filter(|s| now.saturating_duration_since(s.used) >= in_use_for)
        .filter(|s| s.busy_until.is_none_or(|until| until <= now))
        .collect();
    free.sort_by_key(|s| s.used);
    free.into_iter().map(|s| s.desk).collect()
}

/// The fewest of `free` (in that order) that make room: `fits(freed)` with
/// the bytes the closed ones held; `None` = even all of them do not.
pub fn plan_room(free: &[(DeskId, u64)], fits: impl Fn(u64) -> bool) -> Option<Vec<DeskId>> {
    let mut freed = 0;
    let mut closing = Vec::new();
    for (desk, held) in free {
        if fits(freed) {
            return Some(closing);
        }
        freed += held;
        closing.push(*desk);
    }
    fits(freed).then_some(closing)
}

impl Ede {
    /// The spaces as [`giving_way`] reads them.
    fn space_candidates(&self) -> Vec<Candidate> {
        self.spaces.iter().map(|s| Candidate { desk: s.desk.id, used: s.last_use.unwrap_or(s.created), busy_until: s.busy_until }).collect()
    }

    /// Room for one new space: within the count and, where launches run in
    /// slices, under the memory fence; the spaces that must give way for
    /// it close first. `Err` = `limit`, and none closed.
    pub(crate) fn space_make_room(&mut self, max: usize, session: &SessionId) -> Result<(), Failure> {
        let now = Instant::now();
        let free = giving_way(&self.space_candidates(), now, self.spaces.in_use_for);
        let open = self.spaces.iter().count();
        let by_count = (open + 1).saturating_sub(max);
        if by_count > free.len() {
            return Err(Failure::new(ErrorCode::Limit, &format!("{max} spaces at most")));
        }
        let mut closing: Vec<DeskId> = free[..by_count].to_vec();
        if let Some(units) = &self.spaces.units {
            let stat = std::fs::read_to_string(units.all_cgroup().join("memory.stat")).ok();
            let held = |desk: DeskId| std::fs::read_to_string(units.space_cgroup(desk.number()).join("memory.stat")).map_or(0, |stat| super::slice::held(&stat));
            // What the count closes counts as freed; the rest by age.
            let counted: u64 = closing.iter().map(|desk| held(*desk)).sum();
            let rest: Vec<(DeskId, u64)> = free[by_count..].iter().map(|desk| (*desk, held(*desk))).collect();
            let (spaces, machine) = (&self.config.spaces, units.machine);
            let fits = |freed: u64| super::slice::room_freeing(spaces, machine, stat.as_deref(), counted + freed).is_ok();
            match plan_room(&rest, fits) {
                Some(more) => closing.extend(more),
                None => return Err(Failure::new(ErrorCode::Limit, &super::slice::room_freeing(spaces, machine, stat.as_deref(), counted).err().unwrap_or_default())),
            }
        }
        for desk in closing {
            if let Some(space) = self.spaces.get(desk) {
                tracing::info!("{}: gives way to a new space for {} (unused longest, no turn running)", space.id.as_str(), session.as_str());
            }
            self.close_space(desk);
        }
        Ok(())
    }

    /// `space_busy` of the space `desk`: it does not give way for
    /// [`super::Spaces::busy_for`].
    pub(crate) fn space_mark_busy(&mut self, desk: DeskId) {
        let busy_for = self.spaces.busy_for;
        if let Some(space) = self.spaces.get_mut(desk) {
            space.busy_until = Some(Instant::now() + busy_for);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, secs: u64) -> Instant {
        base + Duration::from_secs(secs)
    }

    #[test]
    fn the_space_unused_longest_gives_way_never_one_in_use_or_busy() {
        let base = Instant::now();
        let now = at(base, 3600);
        let desk = DeskId::numbered;
        let spaces = [
            Candidate { desk: desk(1), used: at(base, 600), busy_until: None },
            // Used 5 minutes ago: in use.
            Candidate { desk: desk(2), used: at(base, 3300), busy_until: None },
            // The oldest, but its turn runs.
            Candidate { desk: desk(3), used: base, busy_until: Some(at(base, 3650)) },
            Candidate { desk: desk(4), used: at(base, 100), busy_until: None },
            // Busy once, long ago.
            Candidate { desk: desk(5), used: at(base, 200), busy_until: Some(at(base, 300)) },
        ];
        assert_eq!(giving_way(&spaces, now, IN_USE_FOR), [desk(4), desk(5), desk(1)]);
        assert_eq!(giving_way(&spaces[1..3], now, IN_USE_FOR), Vec::<DeskId>::new(), "every space in use or busy");
    }

    #[test]
    fn for_memory_the_fewest_give_way_or_none() {
        let desk = DeskId::numbered;
        let free = [(desk(4), 300), (desk(5), 100), (desk(1), 500)];
        assert_eq!(plan_room(&free, |_| true), Some(vec![]), "it fits already");
        assert_eq!(plan_room(&free, |freed| freed >= 250), Some(vec![desk(4)]));
        assert_eq!(plan_room(&free, |freed| freed >= 350), Some(vec![desk(4), desk(5)]));
        assert_eq!(plan_room(&free, |freed| freed >= 900), Some(vec![desk(4), desk(5), desk(1)]));
        assert_eq!(plan_room(&free, |freed| freed >= 901), None, "even all of them do not make room: none closes");
    }
}
