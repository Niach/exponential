//! #EDE-141: a store's read (BLOCKING SQLite) off the main loop, one at a
//! time: a read asked for while one runs is not started, it runs once the
//! running one is in. A burst of deltas so costs two reads at most, and the
//! last one starts after the last delta: what is drawn ends at the newest
//! rows. The tray's inbox (#EDE-141) and the rail's chats (#EDE-204).

use std::{cell::RefCell, rc::Rc};

use gpui::App;

/// One read's turn: running, and whether another was asked for meanwhile.
#[derive(Debug, Default)]
pub struct Gate {
    reading: bool,
    again: bool,
}

impl Gate {
    /// `true`: start the read now. `false`: one runs, this ask follows it.
    fn ask(&mut self) -> bool {
        if self.reading {
            self.again = true;
            return false;
        }
        self.reading = true;
        true
    }

    /// The running read is in: `true` = asked for meanwhile, read again.
    fn done(&mut self) -> bool {
        self.reading = false;
        std::mem::take(&mut self.again)
    }
}

/// Reads off the main loop unless a read of `gate` runs (then once more
/// after it, through `again`). `prepare` runs on the main loop and hands
/// back the read itself, which runs on the background executor and never
/// sees `state`; `apply` takes what it read, on the main loop again.
pub fn read<S, T, R>(state: &Rc<RefCell<S>>, gate: fn(&mut S) -> &mut Gate, prepare: impl FnOnce(&Rc<RefCell<S>>) -> R, apply: impl FnOnce(&Rc<RefCell<S>>, T, &mut App) + 'static, again: fn(&Rc<RefCell<S>>, &mut App), cx: &mut App)
where
    S: 'static,
    T: Send + 'static,
    R: FnOnce() -> T + Send + 'static,
{
    if !gate(&mut state.borrow_mut()).ask() {
        return;
    }
    let read = prepare(state);
    let state = state.clone();
    cx.spawn(async move |cx| {
        let out = cx.background_executor().spawn(async move { read() }).await;
        let asked = gate(&mut state.borrow_mut()).done();
        cx.update(|cx| {
            apply(&state, out, cx);
            if asked {
                again(&state, cx);
            }
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use serde_json::{json, Value};
    use sync::{
        protocol::{RowKey, ShapeMessage},
        store::ShapeStore,
    };
    use uicore_model::chats::ChatInput;

    use super::*;
    use crate::chats;

    const TEAM: &str = "t1";

    /// What the shell holds of the rail, and the reads the test counts.
    struct Rail {
        gate: Gate,
        chats: Vec<ChatInput>,
        store: Arc<ShapeStore>,
        reads: Arc<AtomicUsize>,
        /// A delta that lands in the store while the first read runs.
        late: Option<Value>,
    }

    fn put(store: &ShapeStore, row: Value) {
        let key = RowKey::Single(row["id"].as_str().unwrap().to_string());
        store.apply_batch(sync::shapes::shape_by_name("sessions").unwrap(), &[ShapeMessage::Insert { key, value: row.as_object().unwrap().clone() }], None).unwrap();
    }

    fn session(n: usize, title: &str) -> Value {
        json!({"id": format!("r{n:03}"), "team_id": TEAM, "title": title, "cli": "claude", "status": "active", "updated_at": "2026-11-24T10:00:00.000Z"})
    }

    /// The shell's `chats_changed`, on the test's state: the rail's own read
    /// ([`chats::Source::read`]) through [`read`].
    fn chats_changed(rail: &Rc<RefCell<Rail>>, cx: &mut App) {
        read(
            rail,
            |r| &mut r.gate,
            |rail| {
                let mut r = rail.borrow_mut();
                let (source, reads, late) = (chats::Source::Team { store: r.store.clone(), team_id: TEAM.into() }, r.reads.clone(), r.late.take());
                let store = r.store.clone();
                move || {
                    let chats = source.read();
                    reads.fetch_add(1, Ordering::SeqCst);
                    if let Some(row) = late {
                        put(&store, row);
                    }
                    chats
                }
            },
            |rail, chats, _| rail.borrow_mut().chats = chats,
            chats_changed,
            cx,
        );
    }

    fn rail(late: Option<Value>) -> (tempfile::TempDir, Rc<RefCell<Rail>>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ShapeStore::open(&dir.path().join("sync.sqlite")).unwrap());
        (dir, Rc::new(RefCell::new(Rail { gate: Gate::default(), chats: Vec::new(), store, reads: Arc::default(), late })))
    }

    fn titles(rail: &Rc<RefCell<Rail>>) -> Vec<String> {
        let mut titles: Vec<String> = rail.borrow().chats.iter().map(|c| c.title.clone()).collect();
        titles.sort();
        titles
    }

    #[test]
    fn a_read_asked_for_while_one_runs_follows_it_once() {
        let mut gate = Gate::default();
        assert!(gate.ask(), "nothing runs: the read starts");
        assert!(!gate.ask() && !gate.ask() && !gate.ask(), "one runs: none of the asks starts another");
        assert!(gate.done(), "asked for meanwhile: once more");
        assert!(gate.ask(), "the read that follows");
        assert!(!gate.done(), "nobody asked while it ran: no third read");
    }

    /// #EDE-204: before, every delta of the account read every row of
    /// `sessions` and `session_reads` on the main loop (200 deltas = 200
    /// reads inside the loop's own turns).
    #[gpui::test]
    fn a_burst_of_deltas_reads_the_rail_twice_at_most_off_the_main_loop(cx: &mut gpui::TestAppContext) {
        const BURST: usize = 200;
        // The last delta lands while the first read runs, after its rows were read.
        let (_dir, rail) = rail(Some(session(BURST, "The last one")));
        for n in 0..BURST {
            put(&rail.borrow().store, session(n, &format!("Chat {n:03}")));
            cx.update(|cx| chats_changed(&rail, cx));
        }
        assert_eq!(rail.borrow().reads.load(Ordering::SeqCst), 0, "no read ran in the main loop's turns: the store is read on the background executor");
        assert!(rail.borrow().chats.is_empty(), "nothing is drawn before a read is in");

        cx.run_until_parked();
        assert_eq!(rail.borrow().reads.load(Ordering::SeqCst), 2, "{BURST} deltas: the read that ran and the one that followed it");
        let mut wanted: Vec<String> = (0..BURST).map(|n| format!("Chat {n:03}")).collect();
        wanted.push("The last one".into());
        wanted.sort();
        assert_eq!(titles(&rail), wanted, "the rail ends at the newest rows, the delta that landed during the first read too");
    }

    #[gpui::test]
    fn a_single_delta_reads_the_rail_once(cx: &mut gpui::TestAppContext) {
        let (_dir, rail) = rail(None);
        put(&rail.borrow().store, session(1, "First"));
        cx.update(|cx| chats_changed(&rail, cx));
        cx.run_until_parked();
        assert_eq!((rail.borrow().reads.load(Ordering::SeqCst), titles(&rail)), (1, vec!["First".to_string()]));

        // The gate is free again: the next delta reads at once.
        put(&rail.borrow().store, session(2, "Second"));
        cx.update(|cx| chats_changed(&rail, cx));
        cx.run_until_parked();
        assert_eq!((rail.borrow().reads.load(Ordering::SeqCst), titles(&rail)), (2, vec!["First".to_string(), "Second".to_string()]));
    }
}
