//! EXP-825 — an action's typed PICK inputs on the Home composer: the
//! per-key pick maps, the definition-ordered value snapshot and the one
//! field renderer. EXP-1294: the pick types are repo, board, pr and issue
//! (contract `actionInputType`); a `pr` pick is a pull request row. Ported
//! out of the deleted start-coding dialog minus its `text`/`textarea`
//! branches: those input types are retired (the composer's free text reaches
//! every run as its additional-instructions section), and a stale row still
//! declaring one is BLOCKED until the owner removes it on the web — the
//! desktop has no inputs editor.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use gpui::{
    div, AnyElement, App, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
};
use gpui_component::{
    button::Button,
    menu::DropdownMenu as _,
    ActiveTheme as _,
};
use sync::Store;

use crate::action_run::ActionRepoRow;

/// The input types a run can still be handed (contract `actionInputType`).
pub(crate) const PICK_INPUT_TYPES: &[&str] = domain::contract::ACTION_INPUT_TYPE_VALUES;

/// The retired free-text types (EXP-825). A row declaring one is a stale
/// definition, not a schema this build predates.
const RETIRED_INPUT_TYPES: [&str; 2] = ["text", "textarea"];

/// The blocker for a row still carrying a retired free-text input.
pub(crate) const FREE_TEXT_BLOCKER: &str =
    "This action still declares free-text inputs. Remove them on the web.";

/// The blocker for an input type this build does not know.
pub(crate) const NEWER_APP_BLOCKER: &str = "This action needs a newer app version.";

/// Why an action's input SCHEMA cannot be run by this build — `None` when
/// every input is a known pick type. Free text is named separately from a
/// truly unknown type: the first is fixed on the web, the second by updating.
pub(crate) fn unsupported_reason(action: &api::actions::Action) -> Option<&'static str> {
    if action
        .inputs
        .iter()
        .any(|input| RETIRED_INPUT_TYPES.contains(&input.input_type.as_str()))
    {
        return Some(FREE_TEXT_BLOCKER);
    }
    if action
        .inputs
        .iter()
        .any(|input| !PICK_INPUT_TYPES.contains(&input.input_type.as_str()))
    {
        return Some(NEWER_APP_BLOCKER);
    }
    None
}

/// The `pr` input's pick list: the team's OPEN pull requests, `(pull request
/// id, "#n · EXP-1 +2 title")`.
pub(crate) fn pr_pick_options(cx: &App, team_id: &str) -> Vec<(String, String)> {
    let snapshot = crate::session_data::team_pr_snapshot(team_id, cx);
    crate::session_data::open_pull_requests_in_team(team_id, cx)
        .into_iter()
        .map(|pr| {
            let label = snapshot.with_set(|set| {
                let name = set.label(&pr);
                let title = pr
                    .title
                    .clone()
                    .or_else(|| set.representative(&pr).map(|issue| issue.title.clone()))
                    .unwrap_or_default();
                match pr.number {
                    Some(number) if !name.starts_with('#') => format!("#{number} · {name} {title}"),
                    _ => format!("{name} {title}"),
                }
            });
            (pr.id.clone(), label.trim().to_string())
        })
        .collect()
}

/// The picked values of ONE action's inputs, keyed by input key.
#[derive(Default)]
pub(crate) struct ActionInputPicks {
    /// Picked repo per `repo` input key (absent = none picked).
    repo: HashMap<String, ActionRepoRow>,
    /// Picked `(board id, board name)` per `board` input key.
    board: HashMap<String, (String, String)>,
    /// Picked `(pull request id, display label)` per `pr` input key.
    pr: HashMap<String, (String, String)>,
    /// Picked issue id per `issue` input key.
    issue: HashMap<String, String>,
}

impl ActionInputPicks {
    /// Whether `input` currently holds a usable value.
    pub(crate) fn filled(&self, input: &api::actions::ActionInput) -> bool {
        match input.input_type.as_str() {
            "repo" => self.repo.contains_key(&input.key),
            "board" => self.board.contains_key(&input.key),
            "pr" => self.pr.contains_key(&input.key),
            "issue" => self.issue.contains_key(&input.key),
            _ => false,
        }
    }

    /// The first REQUIRED input without a value — the "Fill in X." blocker.
    pub(crate) fn missing_required<'a>(
        &self,
        action: &'a api::actions::Action,
    ) -> Option<&'a api::actions::ActionInput> {
        action
            .inputs
            .iter()
            .find(|input| input.required && !self.filled(input))
    }

    /// The filled input values, key → id (`sessions.send`'s `inputs`; empty
    /// optional inputs are omitted): repo → repository id, board → board id,
    /// pr → pull request id, issue → issue id.
    pub(crate) fn collect(&self, action: &api::actions::Action) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        for input in &action.inputs {
            let value = match input.input_type.as_str() {
                "repo" => self.repo.get(&input.key).map(|repo| repo.id.clone()),
                "board" => self.board.get(&input.key).map(|(board_id, _)| board_id.clone()),
                "pr" => self.pr.get(&input.key).map(|(pr_id, _)| pr_id.clone()),
                "issue" => self.issue.get(&input.key).cloned(),
                // Retired/unknown types never reach here — the blocker gates.
                _ => None,
            };
            if let Some(value) = value {
                values.insert(input.key.clone(), value);
            }
        }
        values
    }

    /// One typed input field (EXP-257): repo/pr → dropdown-menu buttons over
    /// the team's repos / open pull requests, board / issue → THE board and
    /// issue pickers. `access` reaches the picks on the host view so the
    /// menus can write back.
    #[allow(clippy::too_many_arguments)] // one renderer for four field kinds
    pub(crate) fn render_field<V: Render>(
        &self,
        prefix: &'static str,
        ix: usize,
        input: &api::actions::ActionInput,
        team_id: &str,
        team_repos: &[ActionRepoRow],
        access: fn(&mut V) -> &mut ActionInputPicks,
        cx: &mut gpui::Context<V>,
    ) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let label: SharedString = if input.required {
            input.label.clone().into()
        } else {
            format!("{} (optional)", input.label).into()
        };
        let key = input.key.clone();
        let optional = !input.required;
        let view = cx.entity().downgrade();
        let field: AnyElement = match input.input_type.as_str() {
            "repo" => {
                let pick_label: SharedString = match self.repo.get(&input.key) {
                    Some(repo) => repo.full_name.clone().into(),
                    None => unset_label(optional, "Select repository…"),
                };
                let repos = team_repos.to_vec();
                row_trigger(prefix, ix, pick_label, cx)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        if optional {
                            let view = view.clone();
                            let key = key.clone();
                            menu = menu.item(crate::controls::pointer_label_item("None", false).on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            access(view).repo.remove(&key);
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        for repo in &repos {
                            let view = view.clone();
                            let key = key.clone();
                            let repo = repo.clone();
                            menu = menu.item(
                                crate::controls::pointer_label_item(SharedString::from(repo.full_name.clone()), false)
                                    .on_click(move |_, _, cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |view, cx| {
                                                access(view)
                                                    .repo
                                                    .insert(key.clone(), repo.clone());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
            // EXP-1045: THE board picker — the board's own glyph in its own
            // colour, searchable, like every other board pick. A required
            // input offers no clearing row.
            "board" => {
                let picked = self.board.get(&input.key).cloned();
                let pick_label: SharedString = match &picked {
                    Some((_, name)) => name.clone().into(),
                    None => unset_label(optional, "Select board…"),
                };
                let boards = Store::global(cx).collections().boards_in_team(team_id, cx);
                let trigger = row_trigger(prefix, ix, pick_label, cx).into_any_element();
                let picker_id = SharedString::from(format!("{prefix}-board-{ix}"));
                crate::picker::deferred(move |window, cx| {
                    use crate::picker::board_picker::NO_BOARD_VALUE;
                    let names: std::collections::HashMap<String, String> = boards
                        .iter()
                        .map(|board| (board.id.clone(), board.name.clone()))
                        .collect();
                    let current = picked.map(|(board_id, _)| board_id);
                    let on_change: crate::picker::OnPickerChange<String> =
                        Rc::new(move |values: Vec<String>, _window, cx: &mut App| {
                            let Some(board_id) = values.first().cloned() else {
                                return;
                            };
                            let Some(view) = view.upgrade() else {
                                return;
                            };
                            let name = names.get(&board_id).cloned();
                            view.update(cx, |view, cx| {
                                match name {
                                    Some(name) if board_id != NO_BOARD_VALUE => {
                                        access(view).board.insert(key.clone(), (board_id, name));
                                    }
                                    // The clearing row (optional inputs only).
                                    _ => {
                                        access(view).board.remove(&key);
                                    }
                                }
                                cx.notify();
                            });
                        });
                    if optional {
                        crate::picker::board_picker::optional_board_picker(
                            &boards, current, trigger, on_change,
                        )
                    } else {
                        crate::picker::board_picker::board_picker(
                            &boards, current, trigger, on_change,
                        )
                    }
                    .id(picker_id)
                    .render(window, cx)
                })
                .into_any_element()
            }
            "pr" => {
                let pick_label: SharedString = match self.pr.get(&input.key) {
                    Some((_, label)) => label.clone().into(),
                    None => unset_label(optional, "Select pull request…"),
                };
                let pulls = pr_pick_options(cx, team_id);
                row_trigger(prefix, ix, pick_label, cx)
                    .dropdown_menu(pr_menu(view, key, optional, pulls, access))
                    .into_any_element()
            }
            // EXP-1294: one of the team's issues, through THE issue picker.
            "issue" => {
                let picked = self.issue.get(&input.key).cloned();
                let issues = Store::global(cx).collections().issues_in_team(team_id, cx);
                let pick_label: SharedString = match picked
                    .as_deref()
                    .and_then(|id| issues.iter().find(|issue| issue.id == id))
                {
                    Some(issue) => format!("{} {}", issue.identifier, issue.title).into(),
                    None => unset_label(optional, "Select issue…"),
                };
                let trigger = row_trigger(prefix, ix, pick_label, cx).into_any_element();
                let picker_id = SharedString::from(format!("{prefix}-issue-{ix}"));
                crate::picker::deferred(move |window, cx| {
                    let on_change: crate::picker::OnPickerChange<String> =
                        Rc::new(move |values: Vec<String>, _window, cx: &mut App| {
                            let Some(view) = view.upgrade() else {
                                return;
                            };
                            view.update(cx, |view, cx| {
                                match values.first().cloned() {
                                    Some(issue_id) => {
                                        access(view).issue.insert(key.clone(), issue_id);
                                    }
                                    None => {
                                        access(view).issue.remove(&key);
                                    }
                                }
                                cx.notify();
                            });
                        });
                    crate::picker::issue_picker::issue_picker(
                        &issues,
                        picked.clone(),
                        trigger,
                        on_change,
                    )
                    .id(picker_id)
                    .render(window, cx)
                })
                .into_any_element()
            }
            // A retired or unknown type: named by the blocker, never a fake
            // text field.
            _ => div()
                .text_xs()
                .text_color(muted)
                .child(unsupported_reason_for(&input.input_type))
                .into_any_element(),
        };
        // P72 ×4: ONE grouped picker row — the label leading, the value and
        // its chevron trailing (the action page's repository row).
        crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row(
            label, None, field, cx,
        )])
        .into_any_element()
    }
}

/// A picker row's trailing trigger: the value at 70% + the row chevron, no
/// field chrome ([`crate::trigger_editor::picker_trigger`]).
fn row_trigger(prefix: &'static str, ix: usize, label: SharedString, cx: &App) -> Button {
    crate::trigger_editor::picker_trigger(SharedString::from(format!("{prefix}-{ix}")), label, cx)
}

/// An unset input's value: "None" when it is optional (nothing IS a value),
/// the pick prompt when it is required.
fn unset_label(optional: bool, prompt: &'static str) -> SharedString {
    if optional { "None".into() } else { prompt.into() }
}

/// The `pr` input's dropdown — the open-PR list of [`pr_pick_options`] (a
/// "None" row for an optional input).
fn pr_menu<V: 'static>(
    view: gpui::WeakEntity<V>,
    key: String,
    optional: bool,
    pulls: Vec<(String, String)>,
    access: fn(&mut V) -> &mut ActionInputPicks,
) -> impl Fn(
    gpui_component::menu::PopupMenu,
    &mut gpui::Window,
    &mut gpui::Context<gpui_component::menu::PopupMenu>,
) -> gpui_component::menu::PopupMenu
       + 'static {
    move |mut menu, _window, _cx| {
        if optional {
            let view = view.clone();
            let key = key.clone();
            menu = menu.item(crate::controls::pointer_label_item("None", false).on_click(move |_, _, cx| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |view, cx| {
                        access(view).pr.remove(&key);
                        cx.notify();
                    });
                }
            }));
        }
        if pulls.is_empty() {
            menu = menu.item(crate::controls::pointer_label_item("No open pull requests", true));
        }
        for (pr_id, label) in &pulls {
            let view = view.clone();
            let key = key.clone();
            let pr_id = pr_id.clone();
            let label = label.clone();
            menu = menu.item(crate::controls::pointer_label_item(SharedString::from(label.clone()), false).on_click(
                move |_, _, cx| {
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |view, cx| {
                            access(view)
                                .pr
                                .insert(key.clone(), (pr_id.clone(), label.clone()));
                            cx.notify();
                        });
                    }
                },
            ));
        }
        menu
    }
}

/// The per-field copy for a type the composer cannot render.
fn unsupported_reason_for(input_type: &str) -> &'static str {
    if RETIRED_INPUT_TYPES.contains(&input_type) {
        FREE_TEXT_BLOCKER
    } else {
        NEWER_APP_BLOCKER
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::actions::{Action, ActionInput};

    fn input(key: &str, input_type: &str, required: bool) -> ActionInput {
        ActionInput {
            key: key.to_string(),
            label: key.to_uppercase(),
            input_type: input_type.to_string(),
            required,
            placeholder: None,
        }
    }

    fn action(inputs: Vec<ActionInput>) -> Action {
        let mut action: Action = serde_json::from_value(serde_json::json!({
            "id": "act-1", "teamId": "team-1", "name": "Triage",
        }))
        .expect("a minimal action row");
        action.inputs = inputs;
        action
    }

    fn repo(id: &str, full_name: &str) -> ActionRepoRow {
        ActionRepoRow {
            id: id.to_string(),
            full_name: full_name.to_string(),
        }
    }

    /// EXP-825: a stale row still declaring free text is blocked with the
    /// web-pointing sentence; an unknown type with the update one; the four
    /// pick types pass.
    #[test]
    fn unsupported_reason_distinguishes_retired_from_unknown() {
        let picks = action(vec![
            input("r", "repo", false),
            input("b", "board", false),
            input("p", "pr", false),
            input("i", "issue", false),
        ]);
        assert_eq!(unsupported_reason(&picks), None);
        let stale = action(vec![input("scope", "textarea", true)]);
        assert_eq!(unsupported_reason(&stale), Some(FREE_TEXT_BLOCKER));
        let stale = action(vec![input("n", "text", false), input("r", "repo", false)]);
        assert_eq!(unsupported_reason(&stale), Some(FREE_TEXT_BLOCKER));
        let retired = action(vec![input("x", "icon", true)]);
        assert_eq!(unsupported_reason(&retired), Some(NEWER_APP_BLOCKER));
        assert_eq!(FREE_TEXT_BLOCKER, "This action still declares free-text inputs. Remove them on the web.");
    }

    /// EXP-1294: the send's `inputs` are key → id, empty optionals omitted,
    /// and the first missing REQUIRED input names the blocker.
    #[test]
    fn collect_is_key_to_id() {
        let mut picks = ActionInputPicks::default();
        let action = action(vec![
            input("i", "issue", false),
            input("p", "pr", true),
            input("b", "board", false),
            input("r", "repo", false),
        ]);
        assert_eq!(picks.missing_required(&action).map(|i| i.key.as_str()), Some("p"));
        picks.pr.insert("p".into(), ("pr-9".into(), "#4 · EXP-9 T".into()));
        picks.issue.insert("i".into(), "issue-1".into());
        picks.repo.insert("r".into(), repo("repo-1", "acme/web"));
        assert!(picks.missing_required(&action).is_none());
        let values = picks.collect(&action);
        assert_eq!(values.len(), 3);
        assert_eq!(values.get("p").map(String::as_str), Some("pr-9"));
        assert_eq!(values.get("i").map(String::as_str), Some("issue-1"));
        assert_eq!(values.get("r").map(String::as_str), Some("repo-1"));
        assert!(!values.contains_key("b"));
        assert!(picks.filled(&input("i", "issue", false)));
        assert!(!picks.filled(&input("b", "board", false)));
        // A retired type is never "filled" and never collected.
        assert!(!picks.filled(&input("scope", "textarea", true)));
    }
}
