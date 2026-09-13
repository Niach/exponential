//! EXP-825 — an action's typed PICK inputs on the Agent page composer: the
//! per-key pick maps, the EXP-349 repo seed, the fix-conflicts PR preselect,
//! the definition-ordered value snapshot and the one field renderer. Ported
//! out of the deleted start-coding dialog minus its `text`/`textarea`
//! branches: those input types are retired (the composer's free text reaches
//! every run as its additional-instructions section), and a stale row still
//! declaring one is BLOCKED until the owner removes it on the web — the
//! desktop has no inputs editor.

use std::collections::HashMap;

use gpui::{
    div, AnyElement, App, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
};
use gpui_component::{
    button::Button,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _,
};
use sync::Store;

use coding::ActionInputValue;

use crate::action_run::ActionRepoRow;
use crate::controls::WebControl as _;

/// The input types a run can still be handed (contract `actionInputType`).
pub(crate) const PICK_INPUT_TYPES: [&str; 4] = ["repo", "board", "pr", "icon"];

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

/// The `pr` input's pick list (EXP-259): the team's OPEN issue-linked pull
/// requests, deduped by prUrl (a batch PR shows once; its value is the
/// representative issue's id). Shared by the field's dropdown and the
/// fix-conflicts seed's PR preselect (EXP-313).
pub(crate) fn pr_pick_options(cx: &App, team_id: &str) -> Vec<(String, String)> {
    crate::queries::review_groups(cx, team_id)
        .iter()
        .flat_map(|group| group.entries.iter())
        .map(|entry| {
            let issue = entry.representative();
            let idents = entry
                .issues
                .iter()
                .map(|issue| issue.identifier.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let label = match (issue.pr_number, entry.is_batch()) {
                (Some(number), true) => format!("#{number} · {idents}"),
                (Some(number), false) => {
                    format!("#{number} · {idents} {}", issue.title)
                }
                (None, _) => idents,
            };
            (issue.id.clone(), label)
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
    /// Picked `(representative issue id, display label)` per `pr` input key
    /// (EXP-259 — the fix-conflicts builtin's open-PR target).
    pr: HashMap<String, (String, String)>,
    /// Picked curated icon NAME per `icon` input key (EXP-273). The only
    /// pick that is not an id — the value goes to the server verbatim.
    icon: HashMap<String, String>,
    /// Last action id whose `repo` inputs were seeded from the action's own
    /// bound repository (EXP-349) — the latch keeps a manual re-pick
    /// (including clearing to "None") from being re-seeded.
    seeded_repo_action_id: Option<String>,
}

impl ActionInputPicks {
    /// Pre-fill `repo` inputs with the action's bound repository (EXP-349)
    /// — a picker reading "None" while the run targets the bound repo anyway
    /// looked misconfigured. Call on selection and again when
    /// `repositories.list` lands (whichever comes last); the id latch keeps a
    /// manual re-pick from being stomped. Returns whether the seed landed.
    pub(crate) fn seed_repo_inputs(
        &mut self,
        action: &api::actions::Action,
        team_repos: &[ActionRepoRow],
    ) -> bool {
        if self.seeded_repo_action_id.as_deref() == Some(action.id.as_str()) {
            return false;
        }
        let repo_keys: Vec<String> = action
            .inputs
            .iter()
            .filter(|input| input.input_type == "repo")
            .map(|input| input.key.clone())
            .collect();
        let Some(repository_id) = action.repository_id.as_deref() else {
            self.seeded_repo_action_id = Some(action.id.clone());
            return false;
        };
        // Repos not fetched yet — leave the latch unset so the fetch's
        // completion arm retries the seed.
        let Some(repo) = team_repos.iter().find(|repo| repo.id == repository_id).cloned()
        else {
            return false;
        };
        for key in repo_keys {
            self.repo.insert(key, repo.clone());
        }
        self.seeded_repo_action_id = Some(action.id.clone());
        true
    }

    /// EXP-313: fill the action's `pr` input with `issue_id` when that PR is
    /// in the open-reviews list (`options` = [`pr_pick_options`]). A PR that
    /// is not there (racing a merge) is dropped; the user picks manually.
    pub(crate) fn preselect_pr(
        &mut self,
        action: &api::actions::Action,
        issue_id: &str,
        options: &[(String, String)],
    ) -> bool {
        let Some(pick) = options.iter().find(|(id, _)| id == issue_id).cloned() else {
            return false;
        };
        let Some(key) = action
            .inputs
            .iter()
            .find(|input| input.input_type == "pr")
            .map(|input| input.key.clone())
        else {
            return false;
        };
        self.pr.insert(key, pick);
        true
    }

    /// EXP-273: a suggestion's icon seeds the (first) `icon` input.
    pub(crate) fn preselect_icon(&mut self, action: &api::actions::Action, icon: &str) -> bool {
        let Some(key) = action
            .inputs
            .iter()
            .find(|input| input.input_type == "icon")
            .map(|input| input.key.clone())
        else {
            return false;
        };
        self.icon.insert(key, icon.to_string());
        true
    }

    /// Whether `input` currently holds a usable value.
    pub(crate) fn filled(&self, input: &api::actions::ActionInput) -> bool {
        match input.input_type.as_str() {
            "repo" => self.repo.contains_key(&input.key),
            "board" => self.board.contains_key(&input.key),
            "pr" => self.pr.contains_key(&input.key),
            "icon" => self.icon.contains_key(&input.key),
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

    /// Snapshot the filled input values in DEFINITION order (empty optional
    /// inputs are omitted): repo → value=id, display=fullName; board →
    /// value=id, display=name; pr → value=issue id, display=label; icon →
    /// value=display=name.
    pub(crate) fn collect(&self, action: &api::actions::Action) -> Vec<ActionInputValue> {
        let mut values = Vec::new();
        for input in &action.inputs {
            let (value, display) = match input.input_type.as_str() {
                "repo" => match self.repo.get(&input.key) {
                    Some(repo) => (repo.id.clone(), repo.full_name.clone()),
                    None => continue,
                },
                "board" => match self.board.get(&input.key) {
                    Some((board_id, name)) => (board_id.clone(), name.clone()),
                    None => continue,
                },
                "pr" => match self.pr.get(&input.key) {
                    Some((issue_id, label)) => (issue_id.clone(), label.clone()),
                    None => continue,
                },
                "icon" => match self.icon.get(&input.key) {
                    Some(icon) => (icon.clone(), icon.clone()),
                    None => continue,
                },
                // Retired/unknown types never reach here — the blocker gates.
                _ => continue,
            };
            values.push(ActionInputValue {
                key: input.key.clone(),
                label: input.label.clone(),
                input_type: input.input_type.clone(),
                value,
                display: Some(display),
            });
        }
        values
    }

    /// One typed input field (EXP-257): repo/board/pr → dropdown-menu buttons
    /// over the team's repos / synced boards / open PRs, icon → the shared
    /// swatch-and-popover picker. `access` reaches the picks on the host view
    /// so the menus can write back.
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
                    None => "Select repository…".into(),
                };
                let repos = team_repos.to_vec();
                Button::new((prefix, ix))
                    .outline()
                    .cursor_pointer()
                    .web_input_sm()
                    .label(pick_label)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        if optional {
                            let view = view.clone();
                            let key = key.clone();
                            menu = menu.item(PopupMenuItem::new("None").on_click(
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
                                PopupMenuItem::new(SharedString::from(repo.full_name.clone()))
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
            "board" => {
                let pick_label: SharedString = match self.board.get(&input.key) {
                    Some((_, name)) => name.clone().into(),
                    None => "Select board…".into(),
                };
                let boards: Vec<(String, String)> = Store::global(cx)
                    .collections()
                    .boards_in_team(team_id, cx)
                    .into_iter()
                    .map(|board| (board.id, board.name))
                    .collect();
                Button::new((prefix, ix))
                    .outline()
                    .cursor_pointer()
                    .web_input_sm()
                    .label(pick_label)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        if optional {
                            let view = view.clone();
                            let key = key.clone();
                            menu = menu.item(PopupMenuItem::new("None").on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            access(view).board.remove(&key);
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        for (board_id, name) in &boards {
                            let view = view.clone();
                            let key = key.clone();
                            let board_id = board_id.clone();
                            let name = name.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(name.clone())).on_click(
                                    move |_, _, cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |view, cx| {
                                                access(view).board.insert(
                                                    key.clone(),
                                                    (board_id.clone(), name.clone()),
                                                );
                                                cx.notify();
                                            });
                                        }
                                    },
                                ),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
            "pr" => {
                let pick_label: SharedString = match self.pr.get(&input.key) {
                    Some((_, label)) => label.clone().into(),
                    None => "Select pull request…".into(),
                };
                let pulls = pr_pick_options(cx, team_id);
                Button::new((prefix, ix))
                    .outline()
                    .cursor_pointer()
                    .web_input_sm()
                    .label(pick_label)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        if optional {
                            let view = view.clone();
                            let key = key.clone();
                            menu = menu.item(PopupMenuItem::new("None").on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            access(view).pr.remove(&key);
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        if pulls.is_empty() {
                            menu = menu
                                .item(PopupMenuItem::new("No open pull requests").disabled(true));
                        }
                        for (issue_id, label) in &pulls {
                            let view = view.clone();
                            let key = key.clone();
                            let issue_id = issue_id.clone();
                            let label = label.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(label.clone())).on_click(
                                    move |_, _, cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |view, cx| {
                                                access(view).pr.insert(
                                                    key.clone(),
                                                    (issue_id.clone(), label.clone()),
                                                );
                                                cx.notify();
                                            });
                                        }
                                    },
                                ),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
            // EXP-273: the curated icon set. Unlike the other pickers the
            // value is a NAME, not an id. EXP-575: the shared swatch-and-popover
            // picker (`board_form::icon_picker`), as everywhere else.
            "icon" => {
                let picked = self.icon.get(&input.key).cloned();
                crate::board_form::icon_picker(
                    format!("{prefix}-icon-{ix}"),
                    picked.as_deref(),
                    None,
                    optional,
                    move |name, _, cx| {
                        if let Some(view) = view.upgrade() {
                            view.update(cx, |view, cx| {
                                match name {
                                    Some(name) => {
                                        access(view).icon.insert(key.clone(), name.to_string());
                                    }
                                    None => {
                                        access(view).icon.remove(&key);
                                    }
                                }
                                cx.notify();
                            });
                        }
                    },
                    cx,
                )
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
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(muted).child(label))
            .child(field)
            .into_any_element()
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

    fn action(inputs: Vec<ActionInput>, repository_id: Option<&str>) -> Action {
        let mut action = api::actions::builtin_fix_conflicts_action("team-1");
        action.id = "act-1".to_string();
        action.repository_id = repository_id.map(str::to_string);
        action.inputs = inputs;
        action
    }

    fn repo(id: &str, full_name: &str) -> ActionRepoRow {
        ActionRepoRow {
            id: id.to_string(),
            full_name: full_name.to_string(),
            default_branch: None,
        }
    }

    /// EXP-825: a stale row still declaring free text is blocked with the
    /// web-pointing sentence; an unknown type with the update one; the four
    /// pick types pass.
    #[test]
    fn unsupported_reason_distinguishes_retired_from_unknown() {
        let picks = action(
            vec![
                input("r", "repo", false),
                input("b", "board", false),
                input("p", "pr", false),
                input("i", "icon", false),
            ],
            None,
        );
        assert_eq!(unsupported_reason(&picks), None);
        let stale = action(vec![input("scope", "textarea", true)], None);
        assert_eq!(unsupported_reason(&stale), Some(FREE_TEXT_BLOCKER));
        let stale = action(vec![input("n", "text", false), input("r", "repo", false)], None);
        assert_eq!(unsupported_reason(&stale), Some(FREE_TEXT_BLOCKER));
        let newer = action(vec![input("x", "label", true)], None);
        assert_eq!(unsupported_reason(&newer), Some(NEWER_APP_BLOCKER));
        assert_eq!(FREE_TEXT_BLOCKER, "This action still declares free-text inputs. Remove them on the web.");
    }

    /// EXP-349: the repo seed fills every `repo` input from the action's
    /// binding once the repos are known, and the latch keeps a later clear
    /// from being re-seeded.
    #[test]
    fn repo_seed_waits_for_repos_and_latches() {
        let mut picks = ActionInputPicks::default();
        let action = action(vec![input("r", "repo", false)], Some("repo-1"));
        // Repos not fetched: nothing seeded, latch unset → a retry may land.
        assert!(!picks.seed_repo_inputs(&action, &[]));
        assert!(picks.seeded_repo_action_id.is_none());
        let repos = vec![repo("repo-1", "acme/web")];
        assert!(picks.seed_repo_inputs(&action, &repos));
        assert_eq!(picks.repo.get("r").map(|r| r.id.as_str()), Some("repo-1"));
        // A manual clear survives the next seed call.
        picks.repo.remove("r");
        assert!(!picks.seed_repo_inputs(&action, &repos));
        assert!(picks.repo.get("r").is_none());
        // An unbound action latches without seeding.
        let mut picks = ActionInputPicks::default();
        let unbound = self::action(vec![input("r", "repo", false)], None);
        assert!(!picks.seed_repo_inputs(&unbound, &repos));
        assert_eq!(picks.seeded_repo_action_id.as_deref(), Some("act-1"));
    }

    /// The value snapshot follows DEFINITION order, omits empty optionals,
    /// and carries the id + display pairs the prompt renders.
    #[test]
    fn collect_is_definition_ordered_with_displays() {
        let mut picks = ActionInputPicks::default();
        let action = action(
            vec![
                input("i", "icon", false),
                input("p", "pr", true),
                input("b", "board", false),
                input("r", "repo", false),
            ],
            None,
        );
        assert_eq!(picks.missing_required(&action).map(|i| i.key.as_str()), Some("p"));
        assert!(picks.preselect_pr(&action, "issue-9", &[("issue-9".into(), "#4 · EXP-9 T".into())]));
        assert!(!picks.preselect_pr(&action, "gone", &[("issue-9".into(), "x".into())]));
        assert!(picks.preselect_icon(&action, "bug"));
        picks.repo.insert("r".into(), repo("repo-1", "acme/web"));
        assert!(picks.missing_required(&action).is_none());
        let values = picks.collect(&action);
        let keys: Vec<&str> = values.iter().map(|v| v.key.as_str()).collect();
        assert_eq!(keys, vec!["i", "p", "r"]);
        assert_eq!(values[1].value, "issue-9");
        assert_eq!(values[1].display.as_deref(), Some("#4 · EXP-9 T"));
        assert_eq!(values[2].display.as_deref(), Some("acme/web"));
        assert_eq!(values[0].value, "bug");
        assert!(picks.filled(&input("i", "icon", false)));
        assert!(!picks.filled(&input("b", "board", false)));
        // A retired type is never "filled" and never collected.
        assert!(!picks.filled(&input("scope", "textarea", true)));
    }
}
