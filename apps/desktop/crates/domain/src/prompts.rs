//! EXP-1230 — the wording and button roles of every confirm and choice
//! prompt, mirrored from `packages/domain-contract/fixtures/prompts.json`
//! (EXP-1215: web `lib/prompts.ts`, iOS `Prompts`, Android `domain/Prompts`)
//! and locked against it by the tests below.
//!
//! A site renders ONE entry through its builder (which fills the entry's
//! params, nothing else) and hands the [`Prompt`] to the native alert
//! (`ui::native_dialog::AlertSpec::from_prompt`), which maps [`Role`]s onto
//! its buttons and `focus` onto the answer Return gives. Prompts whose copy
//! is locked elsewhere (issue-draft, blocked-start, stack-merge-choice,
//! close-pr, the `/clear` confirm, the agent-account sentences, the GitHub
//! disconnect) are not here; they take only the roles and focus rule.

/// The contract's `roles`, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The plain answer that only dismisses (Esc and the ✕ take it too).
    Cancel,
    /// A plain answer that does something.
    Default,
    /// The accent answer, the safe or expected one.
    Primary,
    /// The answer of a plain destructive confirm beside Cancel.
    Destructive,
    /// Destructive text set apart on the leading edge, beside a primary.
    QuietDestructive,
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::Cancel,
        Role::Default,
        Role::Primary,
        Role::Destructive,
        Role::QuietDestructive,
    ];

    /// The fixture's key for the role.
    pub fn key(self) -> &'static str {
        match self {
            Role::Cancel => "cancel",
            Role::Default => "default",
            Role::Primary => "primary",
            Role::Destructive => "destructive",
            Role::QuietDestructive => "quietDestructive",
        }
    }

    pub fn is_destructive(self) -> bool {
        matches!(self, Role::Destructive | Role::QuietDestructive)
    }
}

/// One answer: `id` keys the site's handler, `label` is what it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub role: Role,
}

/// One fixture entry: `copy` = every `title*`/`body*` key with its
/// `{param}` template (fixture order), `params` = the param names,
/// `actions` in DISPLAY order, `focus` = an action id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    pub id: &'static str,
    pub copy: &'static [(&'static str, &'static str)],
    pub params: &'static [&'static str],
    pub actions: &'static [Action],
    pub focus: &'static str,
}

/// A rendered prompt, ready for an alert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub id: &'static str,
    pub title: String,
    pub body: Option<String>,
    pub actions: &'static [Action],
    pub focus: &'static str,
}

impl Prompt {
    /// The action `focus` names (Return's answer).
    pub fn focused(&self) -> Action {
        self.action(self.focus)
    }

    pub fn action(&self, id: &str) -> Action {
        *self
            .actions
            .iter()
            .find(|a| a.id == id)
            .unwrap_or_else(|| panic!("{}: no action {id}", self.id))
    }
}

impl Spec {
    /// The template under `key` (a fixture `title*`/`body*` key).
    pub fn text(&self, key: &str) -> &'static str {
        self.copy
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .unwrap_or_else(|| panic!("{}: no text {key}", self.id))
    }

    /// Fills `title_key` (and `body_key` when given) with `values`.
    pub fn render(&self, title_key: &str, body_key: Option<&str>, values: &[(&str, &str)]) -> Prompt {
        Prompt {
            id: self.id,
            title: fill(self.text(title_key), values),
            body: body_key.map(|key| fill(self.text(key), values)),
            actions: self.actions,
            focus: self.focus,
        }
    }
}

/// Replaces each `{name}` in ONE pass (a value that itself reads `{x}` stays
/// literal); a placeholder without a value stays as written.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let name = &after[..close];
            values
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v, close))
        });
        match value {
            Some((value, close)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

const CANCEL: Action = Action { id: "cancel", label: "Cancel", role: Role::Cancel };

macro_rules! confirm {
    (destructive $id:literal, $label:literal) => {
        &[CANCEL, Action { id: $id, label: $label, role: Role::Destructive }]
    };
    (primary $id:literal, $label:literal) => {
        &[CANCEL, Action { id: $id, label: $label, role: Role::Primary }]
    };
}

pub const DELETE_ISSUE: Spec = Spec {
    id: "delete-issue",
    copy: &[
        ("title", "Delete {identifier}?"),
        ("body", "Its comments and files are deleted with it."),
    ],
    params: &["identifier"],
    actions: confirm!(destructive "delete", "Delete"),
    focus: "cancel",
};

pub const DELETE_ISSUES: Spec = Spec {
    id: "delete-issues",
    copy: &[
        ("titleOne", "Delete 1 issue?"),
        ("titleMany", "Delete {count} issues?"),
        ("bodyOne", "Its comments and files are deleted with it."),
        ("bodyMany", "Their comments and files are deleted with them."),
    ],
    params: &["count"],
    actions: confirm!(destructive "delete", "Delete"),
    focus: "cancel",
};

pub const DELETE_FILE: Spec = Spec {
    id: "delete-file",
    copy: &[
        ("title", "Delete \"{filename}\"?"),
        ("body", "A description or comment that embeds it shows a deleted image note instead."),
    ],
    params: &["filename"],
    actions: confirm!(destructive "delete", "Delete"),
    focus: "cancel",
};

pub const MOVE_ISSUE: Spec = Spec {
    id: "move-issue",
    copy: &[
        ("title", "Move {identifier} to \"{board}\"?"),
        ("body", "It gets a new identifier in that board."),
    ],
    params: &["identifier", "board"],
    actions: confirm!(primary "move", "Move"),
    focus: "move",
};

pub const MERGE_ISSUE_PR: Spec = Spec {
    id: "merge-issue-pr",
    copy: &[
        ("title", "Merge PR #{number}?"),
        ("titleNoNumber", "Merge this pull request?"),
        ("bodyOne", "It is squash-merged."),
        ("bodyMany", "It is squash-merged. It covers {count} issues."),
    ],
    params: &["number", "count"],
    actions: confirm!(primary "merge", "Merge"),
    focus: "merge",
};

pub const MERGE_RUN_PR: Spec = Spec {
    id: "merge-run-pr",
    copy: &[
        ("title", "Merge PR #{number}?"),
        ("titleNoNumber", "Merge this pull request?"),
        ("body", "It is squash-merged. No issue is linked to it."),
    ],
    params: &["number"],
    actions: confirm!(primary "merge", "Merge"),
    focus: "merge",
};

pub const STOP_RUN: Spec = Spec {
    id: "stop-run",
    copy: &[("title", "Stop this run?")],
    params: &[],
    actions: confirm!(destructive "stop", "Stop"),
    focus: "cancel",
};

pub const RESUME_RUN: Spec = Spec {
    id: "resume-run",
    copy: &[
        ("title", "Resume this run on {device}?"),
        ("titleNoDevice", "Resume this run?"),
        ("body", "The agent continues where it stopped, in the same worktree."),
    ],
    params: &["device"],
    actions: confirm!(primary "resume", "Resume"),
    focus: "resume",
};

pub const DELETE_TRIGGER: Spec = Spec {
    id: "delete-trigger",
    copy: &[("title", "Delete this trigger?"), ("body", "Past runs stay in Runs.")],
    params: &[],
    actions: confirm!(destructive "delete", "Delete"),
    focus: "cancel",
};

pub const REMOVE_DEVICE: Spec = Spec {
    id: "remove-device",
    copy: &[
        ("title", "Remove \"{name}\"?"),
        ("body", "A device with the daemon still running registers again on its next heartbeat."),
    ],
    params: &["name"],
    actions: confirm!(destructive "remove", "Remove"),
    focus: "cancel",
};

pub const DELETE_TEAM: Spec = Spec {
    id: "delete-team",
    copy: &[
        ("title", "Delete \"{name}\"?"),
        ("body", "All its boards, issues and files are deleted for good."),
    ],
    params: &["name"],
    actions: confirm!(destructive "delete", "Delete"),
    focus: "cancel",
};

pub const TRASH_BOARD: Spec = Spec {
    id: "trash-board",
    copy: &[
        ("title", "Move \"{name}\" to trash?"),
        (
            "body",
            "After 48 hours it is deleted with all its issues. Until then an owner can restore it in team settings on the web.",
        ),
    ],
    params: &["name"],
    actions: confirm!(destructive "trash", "Move to trash"),
    focus: "cancel",
};

pub const DELETE_LABEL: Spec = Spec {
    id: "delete-label",
    copy: &[
        ("title", "Delete \"{name}\"?"),
        ("body", "It comes off every issue that has it."),
    ],
    params: &["name"],
    actions: confirm!(destructive "delete", "Delete"),
    focus: "cancel",
};

pub const REMOVE_MEMBER: Spec = Spec {
    id: "remove-member",
    copy: &[
        ("title", "Remove {name} from this team?"),
        ("body", "They lose access immediately."),
    ],
    params: &["name"],
    actions: confirm!(destructive "remove", "Remove"),
    focus: "cancel",
};

pub const LEAVE_TEAM: Spec = Spec {
    id: "leave-team",
    copy: &[
        ("title", "Leave \"{name}\"?"),
        ("body", "You need a new invite to come back."),
    ],
    params: &["name"],
    actions: confirm!(destructive "leave", "Leave"),
    focus: "cancel",
};

pub const MAKE_OWNER: Spec = Spec {
    id: "make-owner",
    copy: &[
        ("title", "Make {name} an owner?"),
        ("body", "Owners can manage members and billing, delete boards and delete the team."),
    ],
    params: &["name"],
    actions: confirm!(primary "make-owner", "Make owner"),
    focus: "make-owner",
};

pub const MAKE_MEMBER: Spec = Spec {
    id: "make-member",
    copy: &[
        ("title", "Make {name} a member?"),
        ("body", "They can no longer manage members or delete boards."),
    ],
    params: &["name"],
    actions: confirm!(primary "make-member", "Make member"),
    focus: "make-member",
};

pub const REMOVE_REPOSITORY: Spec = Spec {
    id: "remove-repository",
    copy: &[("title", "Remove {fullName} from this team?")],
    params: &["fullName"],
    actions: confirm!(destructive "remove", "Remove"),
    focus: "cancel",
};

pub const UNLINK_SIGN_IN_METHOD: Spec = Spec {
    id: "unlink-sign-in-method",
    copy: &[
        ("title", "Unlink {provider}?"),
        ("body", "Your other sign-in methods keep working. You can link it again any time."),
    ],
    params: &["provider"],
    actions: confirm!(destructive "unlink", "Unlink"),
    focus: "cancel",
};

pub const REMOVE_PASSWORD: Spec = Spec {
    id: "remove-password",
    copy: &[
        ("title", "Remove your password?"),
        ("body", "Your other sign-in methods keep working."),
    ],
    params: &[],
    actions: confirm!(destructive "remove", "Remove"),
    focus: "cancel",
};

pub const REMOVE_PASSKEY: Spec = Spec {
    id: "remove-passkey",
    copy: &[
        ("title", "Remove the passkey \"{name}\"?"),
        ("body", "The copy on your device stays until you delete it there."),
    ],
    params: &["name"],
    actions: confirm!(destructive "remove", "Remove"),
    focus: "cancel",
};

pub const DELETE_ACCOUNT: Spec = Spec {
    id: "delete-account",
    copy: &[
        ("title", "Delete your account?"),
        ("titleOnServer", "Delete your account on {server}?"),
        (
            "body",
            "Teams where you are the only member are deleted with all their issues. In shared teams your issues stay and your comments are deleted.",
        ),
    ],
    params: &["server"],
    actions: confirm!(destructive "delete", "Delete"),
    focus: "cancel",
};

pub const REMOVE_SERVER: Spec = Spec {
    id: "remove-server",
    copy: &[
        ("title", "Remove {server}?"),
        (
            "body",
            "You are signed out and its data on this device is deleted. You can add it again any time.",
        ),
    ],
    params: &["server"],
    actions: confirm!(destructive "remove", "Remove"),
    focus: "cancel",
};

/// Every entry, in fixture order (the lock test walks it).
pub const ALL: &[Spec] = &[
    DELETE_ISSUE,
    DELETE_ISSUES,
    DELETE_FILE,
    MOVE_ISSUE,
    MERGE_ISSUE_PR,
    MERGE_RUN_PR,
    STOP_RUN,
    RESUME_RUN,
    DELETE_TRIGGER,
    REMOVE_DEVICE,
    DELETE_TEAM,
    TRASH_BOARD,
    DELETE_LABEL,
    REMOVE_MEMBER,
    LEAVE_TEAM,
    MAKE_OWNER,
    MAKE_MEMBER,
    REMOVE_REPOSITORY,
    UNLINK_SIGN_IN_METHOD,
    REMOVE_PASSWORD,
    REMOVE_PASSKEY,
    DELETE_ACCOUNT,
    REMOVE_SERVER,
];

// ---------------------------------------------------------------------------
// Builders: one per entry, filling its params and picking its variant keys
// exactly like the web/iOS/Android builders.
// ---------------------------------------------------------------------------

pub fn delete_issue(identifier: &str) -> Prompt {
    DELETE_ISSUE.render("title", Some("body"), &[("identifier", identifier)])
}

pub fn delete_issues(count: usize) -> Prompt {
    if count == 1 {
        DELETE_ISSUES.render("titleOne", Some("bodyOne"), &[])
    } else {
        DELETE_ISSUES.render("titleMany", Some("bodyMany"), &[("count", &count.to_string())])
    }
}

pub fn delete_file(filename: &str) -> Prompt {
    DELETE_FILE.render("title", Some("body"), &[("filename", filename)])
}

pub fn move_issue(identifier: &str, board: &str) -> Prompt {
    MOVE_ISSUE.render("title", Some("body"), &[("identifier", identifier), ("board", board)])
}

/// `issue_count` = the issues the PR links (2+ = a batch PR).
pub fn merge_issue_pr(number: Option<u64>, issue_count: usize) -> Prompt {
    let number = number.map(|n| n.to_string());
    MERGE_ISSUE_PR.render(
        if number.is_some() { "title" } else { "titleNoNumber" },
        Some(if issue_count > 1 { "bodyMany" } else { "bodyOne" }),
        &[
            ("number", number.as_deref().unwrap_or("")),
            ("count", &issue_count.to_string()),
        ],
    )
}

pub fn merge_run_pr(number: Option<u64>) -> Prompt {
    let number = number.map(|n| n.to_string());
    MERGE_RUN_PR.render(
        if number.is_some() { "title" } else { "titleNoNumber" },
        Some("body"),
        &[("number", number.as_deref().unwrap_or(""))],
    )
}

pub fn stop_run() -> Prompt {
    STOP_RUN.render("title", None, &[])
}

/// `device` missing or blank = the device-less title.
pub fn resume_run(device: Option<&str>) -> Prompt {
    match device.filter(|d| !d.trim().is_empty()) {
        Some(device) => RESUME_RUN.render("title", Some("body"), &[("device", device)]),
        None => RESUME_RUN.render("titleNoDevice", Some("body"), &[]),
    }
}

pub fn delete_trigger() -> Prompt {
    DELETE_TRIGGER.render("title", Some("body"), &[])
}

pub fn remove_device(name: &str) -> Prompt {
    REMOVE_DEVICE.render("title", Some("body"), &[("name", name)])
}

pub fn delete_team(name: &str) -> Prompt {
    DELETE_TEAM.render("title", Some("body"), &[("name", name)])
}

pub fn trash_board(name: &str) -> Prompt {
    TRASH_BOARD.render("title", Some("body"), &[("name", name)])
}

pub fn delete_label(name: &str) -> Prompt {
    DELETE_LABEL.render("title", Some("body"), &[("name", name)])
}

pub fn remove_member(name: &str) -> Prompt {
    REMOVE_MEMBER.render("title", Some("body"), &[("name", name)])
}

pub fn leave_team(name: &str) -> Prompt {
    LEAVE_TEAM.render("title", Some("body"), &[("name", name)])
}

pub fn make_owner(name: &str) -> Prompt {
    MAKE_OWNER.render("title", Some("body"), &[("name", name)])
}

pub fn make_member(name: &str) -> Prompt {
    MAKE_MEMBER.render("title", Some("body"), &[("name", name)])
}

pub fn remove_repository(full_name: &str) -> Prompt {
    REMOVE_REPOSITORY.render("title", None, &[("fullName", full_name)])
}

pub fn unlink_sign_in_method(provider: &str) -> Prompt {
    UNLINK_SIGN_IN_METHOD.render("title", Some("body"), &[("provider", provider)])
}

pub fn remove_password() -> Prompt {
    REMOVE_PASSWORD.render("title", Some("body"), &[])
}

/// `name` missing or blank = the contract's "Passkey".
pub fn remove_passkey(name: Option<&str>) -> Prompt {
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or("Passkey");
    REMOVE_PASSKEY.render("title", Some("body"), &[("name", name)])
}

/// `server` = the instance's name, `None` = the plain title.
pub fn delete_account(server: Option<&str>) -> Prompt {
    match server {
        Some(server) => DELETE_ACCOUNT.render("titleOnServer", Some("body"), &[("server", server)]),
        None => DELETE_ACCOUNT.render("title", Some("body"), &[]),
    }
}

pub fn remove_server(server: &str) -> Prompt {
    REMOVE_SERVER.render("title", Some("body"), &[("server", server)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/prompts.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    /// Keys of an entry that are not copy.
    const STRUCTURAL: [&str; 5] = ["params", "actions", "focus", "variants", "slot"];

    #[test]
    fn the_roles_are_the_contracts() {
        let fixture = fixture();
        let roles: Vec<&str> = fixture["roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        assert_eq!(roles, Role::ALL.map(Role::key).to_vec());
    }

    #[test]
    fn the_entry_set_is_the_contracts() {
        let fixture = fixture();
        // Object key order is not guaranteed by serde_json: compare as sets.
        let mut ids: Vec<&str> =
            fixture["prompts"].as_object().unwrap().keys().map(String::as_str).collect();
        ids.sort_unstable();
        let mut mine: Vec<&str> = ALL.iter().map(|s| s.id).collect();
        mine.sort_unstable();
        assert_eq!(ids, mine);
    }

    #[test]
    fn every_entrys_copy_params_actions_and_focus_are_the_contracts() {
        let fixture = fixture();
        for spec in ALL {
            let entry = fixture["prompts"][spec.id].as_object().unwrap();
            let mut copy: Vec<(&str, &str)> = entry
                .iter()
                .filter(|(k, _)| !STRUCTURAL.contains(&k.as_str()))
                .map(|(k, v)| (k.as_str(), v.as_str().unwrap()))
                .collect();
            copy.sort_unstable();
            let mut mine = spec.copy.to_vec();
            mine.sort_unstable();
            assert_eq!(copy, mine, "{} copy", spec.id);
            let params: Vec<&str> = entry["params"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["name"].as_str().unwrap())
                .collect();
            assert_eq!(params, spec.params.to_vec(), "{} params", spec.id);
            let actions: Vec<(&str, &str, &str)> = entry["actions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| {
                    (
                        a["id"].as_str().unwrap(),
                        a["label"].as_str().unwrap(),
                        a["role"].as_str().unwrap(),
                    )
                })
                .collect();
            let mine: Vec<(&str, &str, &str)> =
                spec.actions.iter().map(|a| (a.id, a.label, a.role.key())).collect();
            assert_eq!(actions, mine, "{} actions", spec.id);
            assert_eq!(entry["focus"].as_str(), Some(spec.focus), "{} focus", spec.id);
        }
    }

    #[test]
    fn every_placeholder_is_a_declared_param() {
        let placeholder = regex::Regex::new(r"\{(\w+)\}").unwrap();
        for spec in ALL {
            for (_, template) in spec.copy {
                for cap in placeholder.captures_iter(template) {
                    assert!(spec.params.contains(&&cap[1]), "{}: {}", spec.id, &cap[0]);
                }
            }
        }
    }

    #[test]
    fn focus_never_lands_on_a_destructive_answer() {
        for spec in ALL {
            let focused = spec.actions.iter().find(|a| a.id == spec.focus).unwrap();
            assert!(!focused.role.is_destructive(), "{}", spec.id);
        }
    }

    #[test]
    fn fill_is_single_pass() {
        assert_eq!(fill("Delete \"{name}\"?", &[("name", "{name}")]), "Delete \"{name}\"?");
        assert_eq!(fill("{a} {b}", &[("a", "{b}"), ("b", "x")]), "{b} x");
        assert_eq!(fill("{missing} {", &[]), "{missing} {");
    }

    #[test]
    fn the_builders_fill_every_param() {
        let rendered = [
            delete_issue("EXP-12"),
            delete_issues(1),
            delete_issues(3),
            delete_file("a.png"),
            move_issue("EXP-12", "Web"),
            merge_issue_pr(Some(7), 1),
            merge_issue_pr(None, 3),
            merge_run_pr(Some(7)),
            merge_run_pr(None),
            stop_run(),
            resume_run(Some("Mac")),
            resume_run(None),
            delete_trigger(),
            remove_device("Mac"),
            delete_team("Acme"),
            trash_board("Web"),
            delete_label("bug"),
            remove_member("Ada"),
            leave_team("Acme"),
            make_owner("Ada"),
            make_member("Ada"),
            remove_repository("niach/exponential"),
            unlink_sign_in_method("Google"),
            remove_password(),
            remove_passkey(None),
            delete_account(Some("Cloud")),
            delete_account(None),
            remove_server("Cloud"),
        ];
        for prompt in rendered {
            assert!(
                !prompt.title.contains('{') && !prompt.body.as_deref().unwrap_or("").contains('{'),
                "{}",
                prompt.title
            );
        }
    }

    #[test]
    fn the_variants_pick_the_right_keys() {
        assert_eq!(delete_issue("EXP-12").title, "Delete EXP-12?");
        assert_eq!(delete_issues(1).title, "Delete 1 issue?");
        assert_eq!(
            delete_issues(2).body.as_deref(),
            Some("Their comments and files are deleted with them.")
        );
        assert_eq!(delete_issues(2).title, "Delete 2 issues?");
        assert_eq!(merge_issue_pr(Some(7), 1).title, "Merge PR #7?");
        assert_eq!(merge_issue_pr(Some(7), 1).body.as_deref(), Some("It is squash-merged."));
        assert_eq!(
            merge_issue_pr(Some(7), 3).body.as_deref(),
            Some("It is squash-merged. It covers 3 issues.")
        );
        assert_eq!(merge_run_pr(None).title, "Merge this pull request?");
        assert_eq!(resume_run(Some(" ")).title, "Resume this run?");
        assert_eq!(resume_run(Some("Mac")).title, "Resume this run on Mac?");
        assert_eq!(stop_run().body, None);
        assert_eq!(remove_passkey(Some("")).title, "Remove the passkey \"Passkey\"?");
        assert_eq!(delete_account(Some("Cloud")).title, "Delete your account on Cloud?");
        assert_eq!(trash_board("Web").title, "Move \"Web\" to trash?");
        assert_eq!(stop_run().focused().role, Role::Cancel);
        assert_eq!(make_owner("Ada").focused().label, "Make owner");
    }
}
