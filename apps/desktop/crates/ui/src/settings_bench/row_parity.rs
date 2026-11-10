use super::*;

/// #EXP-1629 (genUI Select option `trailing`): the Branch row's menu opened
/// (the IDE's by its dev open, as a click would; genUI's by a trigger
/// press), today's IDE row vs the genUI row: run each into its own dir, the
/// second with `EXP_SETTINGS_BEFORE=<the first's dir>`.
#[test]
#[ignore]
fn settings_branch_menu_ide() {
    shoot_pane("branch-menu", IdeBranchRow::for_shots, |_, _| {});
}

#[test]
#[ignore]
fn settings_branch_menu_eui() {
    shoot_pane("branch-menu", EuiBranchRow::for_shots, |_, _| {});
}

/// #EXP-1764 (genUI Select `triggerIcon` + the `brand-claude` mark): the
/// Agent detail Account row closed (the brand mark, the email, the
/// chevron), today's IDE row vs the genUI row: run each into its own dir,
/// the second with `EXP_SETTINGS_BEFORE=<the first's dir>`.
#[test]
#[ignore]
fn settings_account_row_ide() {
    shoot_pane("account-row", IdeAccountRow::for_shots, |_, _| {});
}

#[test]
#[ignore]
fn settings_account_row_eui() {
    shoot_pane("account-row", EuiAccountRow::for_shots, |_, _| {});
}

/// #EXP-1765: Repositories' per-repo branch picker: the IDE's glass pill
/// (`repositories::branch_trigger` + its `Popover`) vs genUI's `pill`
/// Select, closed (normal, disabled) and open (loading, empty, ready with
/// `main` the default and checked). Shoot `settings_repo_branch_ide`, then
/// `settings_repo_branch_eui` with `EXP_SETTINGS_BEFORE` at the first.
const REPO_BRANCH_STATES: [(&str, PickerState); 5] = [
    ("repo-branch-trigger", PickerState::Trigger),
    ("repo-branch-disabled", PickerState::Disabled),
    ("repo-branch-loading", PickerState::Loading),
    ("repo-branch-empty", PickerState::Empty),
    ("repo-branch-ready", PickerState::Ready),
];

#[test]
#[ignore]
fn settings_repo_branch_ide() {
    for (name, state) in REPO_BRANCH_STATES {
        shoot_pane(name, IdeBranchPicker::for_shots(state), |_, _| {});
    }
}

#[test]
#[ignore]
fn settings_repo_branch_eui() {
    for (name, state) in REPO_BRANCH_STATES {
        shoot_pane(name, EuiBranchPicker::for_shots(state), |_, _| {});
    }
}
