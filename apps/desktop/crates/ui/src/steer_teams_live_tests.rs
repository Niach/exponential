//! #EXP-2122 (composer-menu.json `teams`, `teamsPick.plainCases`): the live
//! session composer of a PLAIN own root (no agent, no app) shows the team
//! chips and the Teams row, a toggle rides the next send, an untouched send
//! leaves `scopeTeamIds` out.

use gpui::{Entity, VisualTestContext};
use serde_json::json;
use sync::Store;

use super::{ComposerEvent, PlusAction, SteerSessionView};

const SESSION: &str = "c81d4e2e-bcf2-11e6-869b-7df92533d2db";

fn seed<T: sync::ShapeRow + serde::de::DeserializeOwned>(collection: &Entity<sync::Collection<T>>, rows: serde_json::Value, cx: &mut gpui::App) {
    collection.update(cx, |c, cx| {
        for row in rows.as_array().unwrap() {
            let row: T = serde_json::from_value(row.clone()).unwrap();
            c.seed(row.key(), row);
        }
        cx.notify();
    });
}

struct Harness<'a> {
    steer: Entity<SteerSessionView>,
    cx: &'a mut VisualTestContext,
    _dir: tempfile::TempDir,
}

/// The viewer (`u1`, teams One / Three / Two) on the session `row` (merged
/// over a plain root they started in team One).
fn harness(row: serde_json::Value, cx: &mut gpui::TestAppContext) -> Harness<'_> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("accounts.json"),
        // A closed port: a send fails at once and never leaves the machine.
        json!({"accounts": [{"id": "acc", "instanceUrl": "http://127.0.0.1:9", "userId": "u1", "email": "me@example.test", "name": "Me"}]}).to_string(),
    )
    .unwrap();
    let db = dir.path().join("local.sqlite");
    let (_changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    let data = dir.path().to_path_buf();
    let mut session = json!({
        "id": SESSION, "team_id": "t1", "root_id": SESSION, "user_id": "u1", "status": "idle", "shared": false, "device_id": "d1",
        "created_at": "2026-11-04T09:00:00.000Z", "updated_at": "2026-11-04T09:00:00.000Z",
    });
    session.as_object_mut().unwrap().extend(row.as_object().unwrap().clone());
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
        crate::init(cx);
        cx.set_global(crate::AuthContext { auth: api::AuthStore::load(data.clone()), client: std::sync::Arc::new(api::AuthClient::new()), data_dir: data });
        cx.set_global(crate::session_outbox::SessionOutbox::in_memory_for_test());
        let store = Store::open(cx, None, None);
        cx.set_global(store);
        Store::global(cx).clone().connect_local("acc", &db, rx, cx).unwrap();
        let c = Store::global(cx).collections().clone();
        seed(&c.teams, json!([{"id": "t1", "name": "One", "slug": "one"}, {"id": "t2", "name": "Two", "slug": "two"}, {"id": "t3", "name": "Three", "slug": "three"}]), cx);
        seed(&c.sessions, json!([session]), cx);
    });
    cx.run_until_parked();
    let (screen, vcx) = cx.add_window_view(|window, cx| crate::session_screen::SessionScreenView::new(SESSION.to_string(), window, cx));
    vcx.run_until_parked();
    let steer = screen.read_with(vcx, |screen, _| screen.inner().clone());
    Harness { steer, cx: vcx, _dir: dir }
}

impl Harness<'_> {
    /// The composer strip's chips.
    fn chips(&mut self) -> Vec<String> {
        let files = self.steer.update(self.cx, |steer, cx| steer.composer_frame(cx).files);
        files["items"].as_array().unwrap().iter().map(|item| item["key"].as_str().unwrap().to_string()).collect()
    }

    /// The "+" › Teams row's checked teams (`None` = no row).
    fn row(&mut self) -> Option<Vec<String>> {
        let rows = self.steer.update(self.cx, |steer, cx| steer.composer_frame(cx).input.plus);
        let row = rows.into_iter().find(|row| row.id == domain::chat_document::PLUS_TEAMS)?;
        Some(row.items.unwrap().into_iter().filter(|item| item.checked == Some(true)).map(|item| item.value).collect())
    }

    fn event(&mut self, event: ComposerEvent) {
        let steer = self.steer.clone();
        self.cx.update(|window, cx| steer.update(cx, |steer, cx| steer.on_composer_event(event, window, cx)));
        self.cx.run_until_parked();
    }

    /// Sends `text`; the `scopeTeamIds` key it went out with.
    fn send(&mut self, text: &str) -> Option<serde_json::Value> {
        self.event(ComposerEvent::Send(text.into()));
        let input = self.cx.update(|_, cx| crate::session_outbox::sent_input_for_test(SESSION, text, cx)).expect("the send went to the outbox");
        assert_eq!(input.session_id.as_deref(), Some(SESSION));
        serde_json::to_value(input).unwrap().as_object().unwrap().get("scopeTeamIds").cloned()
    }
}

#[gpui::test]
fn a_plain_roots_composer_carries_its_teams_pick(cx: &mut gpui::TestAppContext) {
    let mut h = harness(json!({}), cx);
    // Untouched: its own team's chip, checked in the row, and no key on a send.
    assert_eq!(h.chips(), ["team:t1"]);
    assert_eq!(h.row(), Some(vec!["t1".to_string()]));
    assert_eq!(h.send("untouched"), None);

    // A toggle: both chips, and the next send names the OTHER team.
    h.event(ComposerEvent::Plus(PlusAction::Team("t2".into())));
    let mut chips = h.chips();
    chips.sort();
    assert_eq!(chips, ["team:t1", "team:t2"]);
    assert_eq!(h.send("toggled"), Some(json!(["t2"])));

    // Toggled back: what is stored again, so the key goes out no more.
    h.event(ComposerEvent::Unattach("team:t2".into()));
    assert_eq!(h.chips(), ["team:t1"]);
    assert_eq!(h.send("back"), None);

    // Its own team stays, whatever is taken off.
    h.event(ComposerEvent::Unattach("team:t1".into()));
    assert!(h.row().unwrap().contains(&"t1".to_string()));
}

#[gpui::test]
fn a_plain_roots_stored_pick_shows_and_clears(cx: &mut gpui::TestAppContext) {
    let mut h = harness(json!({"scope_team_ids": ["t2"]}), cx);
    let mut chips = h.chips();
    chips.sort();
    assert_eq!(chips, ["team:t1", "team:t2"]);
    assert_eq!(h.send("stored"), None, "the stored pick is never sent again");
    h.event(ComposerEvent::Unattach("team:t2".into()));
    assert_eq!(h.chips(), ["team:t1"]);
    assert_eq!(h.send("cleared"), Some(serde_json::Value::Null), "its own team only");
}

/// Another member's plain root, a team agent's and an app's editor carry no pick.
#[gpui::test]
fn no_teams_pick_off_a_plain_own_root(cx: &mut gpui::TestAppContext) {
    for row in [
        json!({"user_id": "u2", "shared": true}),
        json!({"agent_id": "agent1"}),
        json!({"app_id": "app1"}),
    ] {
        let mut cx = cx.clone();
        let mut h = harness(row.clone(), &mut cx);
        assert_eq!(h.chips(), Vec::<String>::new(), "{row}");
        assert_eq!(h.row(), None, "{row}");
        assert_eq!(h.send("hello"), None, "{row}");
    }
}
