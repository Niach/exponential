import Foundation

/// EXP-1215: the wording of every confirm/choice prompt, byte-identical on
/// web (`packages/ui` Prompt), iOS (`GlassAlert`) and Android, mirrored from
/// the contract fixture `domain-contract/fixtures/prompts.json` and locked by
/// `PromptsTests`. A prompt site renders an entry by id and fills only the
/// `{params}` it names; ExpUI's `GlassAlert(prompt:handlers:)` turns the
/// filled value into the card's row.
///
/// Prompts locked elsewhere keep their own mirror (`IssueDraftPage`,
/// `BlockedStart`, `PrStack`, `ClosePrCopy`, the steer `/clear` confirm,
/// `AgentAccountsRows`, `GithubCopy`).

/// An answer's role (fixture `roles`).
public enum PromptRole: String, CaseIterable, Sendable {
    /// The plain pill that only dismisses (the scrim takes the same path).
    case cancel
    /// A plain pill that does something.
    case `default`
    /// The accent pill: the safe or expected answer.
    case primary
    /// The plain pill with a destructive label and tinted border.
    case destructive
    /// Destructive text set apart on the leading edge, beside a primary.
    case quietDestructive
}

/// One answer, in display order (leading to trailing).
public struct PromptAction: Equatable, Sendable {
    public let id: String
    public let label: String
    public let role: PromptRole

    public init(id: String, label: String, role: PromptRole) {
        self.id = id
        self.label = label
        self.role = role
    }
}

/// A prompt with its placeholders filled: what a card shows.
public struct PromptCopy: Equatable, Sendable {
    public let title: String
    /// nil = no body line.
    public let body: String?
    public let actions: [PromptAction]
    /// The action id that takes initial focus and Return.
    public let focus: String
}

/// One fixture entry, unfilled.
public struct PromptEntry: Sendable {
    public let id: String
    /// Every `title*` / `body*` key with its `{param}` template.
    public let texts: [String: String]
    public let params: [String]
    public let actions: [PromptAction]
    public let focus: String

    /// Fills `titleKey` and `bodyKey` (absent key = no body).
    func copy(
        title titleKey: String = "title",
        body bodyKey: String = "body",
        _ values: [String: String] = [:]
    ) -> PromptCopy {
        PromptCopy(
            title: Prompts.fill(texts[titleKey] ?? "", values),
            body: texts[bodyKey].map { Prompts.fill($0, values) },
            actions: actions,
            focus: focus
        )
    }
}

public enum Prompts {
    /// Replaces every `{name}` with its value in ONE pass over the template,
    /// so a value that itself contains a `{param}` is never re-filled and the
    /// dictionary's order never changes the output; an unnamed placeholder
    /// stays as typed.
    public static func fill(_ template: String, _ values: [String: String]) -> String {
        guard !values.isEmpty, template.contains("{") else { return template }
        // `{name}`: a word (letters, digits, underscore) in braces. Built per
        // call: `Regex` is not `Sendable`, so it cannot be a static.
        let placeholder = #/\{(\w+)\}/#
        var output = ""
        var cursor = template.startIndex
        for match in template.matches(of: placeholder) {
            output += template[cursor..<match.range.lowerBound]
            output += values[String(match.output.1)] ?? String(template[match.range])
            cursor = match.range.upperBound
        }
        output += template[cursor...]
        return output
    }

    private static let cancel = PromptAction(id: "cancel", label: "Cancel", role: .cancel)

    private static func confirm(_ id: String, _ label: String, _ role: PromptRole) -> [PromptAction] {
        [cancel, PromptAction(id: id, label: label, role: role)]
    }

    /// A plain destructive confirm: Cancel + the destructive answer, Cancel
    /// focused.
    private static func destructive(
        _ id: String,
        texts: [String: String],
        params: [String] = [],
        answer: String,
        label: String
    ) -> PromptEntry {
        PromptEntry(
            id: id, texts: texts, params: params,
            actions: confirm(answer, label, .destructive), focus: "cancel"
        )
    }

    /// Cancel + a primary answer, the primary focused.
    private static func primary(
        _ id: String,
        texts: [String: String],
        params: [String] = [],
        answer: String,
        label: String
    ) -> PromptEntry {
        PromptEntry(
            id: id, texts: texts, params: params,
            actions: confirm(answer, label, .primary), focus: answer
        )
    }

    // MARK: - Entries

    public enum DeleteIssue {
        public static let entry = destructive(
            "delete-issue",
            texts: [
                "title": "Delete {identifier}?",
                "body": "Its comments and files are deleted with it.",
            ],
            params: ["identifier"], answer: "delete", label: "Delete"
        )
        public static func copy(identifier: String) -> PromptCopy {
            entry.copy(["identifier": identifier])
        }
    }

    public enum DeleteIssues {
        public static let entry = destructive(
            "delete-issues",
            texts: [
                "titleOne": "Delete 1 issue?",
                "titleMany": "Delete {count} issues?",
                "bodyOne": "Its comments and files are deleted with it.",
                "bodyMany": "Their comments and files are deleted with them.",
            ],
            params: ["count"], answer: "delete", label: "Delete"
        )
        public static func copy(count: Int) -> PromptCopy {
            let one = count == 1
            return entry.copy(
                title: one ? "titleOne" : "titleMany",
                body: one ? "bodyOne" : "bodyMany",
                ["count": "\(count)"]
            )
        }
    }

    public enum DeleteFile {
        public static let entry = destructive(
            "delete-file",
            texts: [
                "title": "Delete \"{filename}\"?",
                "body": "A description or comment that embeds it shows a deleted image note instead.",
            ],
            params: ["filename"], answer: "delete", label: "Delete"
        )
        public static func copy(filename: String) -> PromptCopy {
            entry.copy(["filename": filename])
        }
    }

    public enum MoveIssue {
        public static let entry = primary(
            "move-issue",
            texts: [
                "title": "Move {identifier} to \"{board}\"?",
                "body": "It gets a new identifier in that board.",
            ],
            params: ["identifier", "board"], answer: "move", label: "Move"
        )
        public static func copy(identifier: String, board: String) -> PromptCopy {
            entry.copy(["identifier": identifier, "board": board])
        }
    }

    public enum MergeIssuePr {
        public static let entry = primary(
            "merge-issue-pr",
            texts: [
                "title": "Merge PR #{number}?",
                "titleNoNumber": "Merge this pull request?",
                "bodyOne": "It is squash-merged.",
                "bodyMany": "It is squash-merged. It covers {count} issues.",
            ],
            params: ["number", "count"], answer: "merge", label: "Merge"
        )
        /// `issueCount` = the issues the pull request links (2+ = a batch).
        public static func copy(number: Int?, issueCount: Int) -> PromptCopy {
            var values = ["count": "\(issueCount)"]
            if let number { values["number"] = "\(number)" }
            return entry.copy(
                title: number == nil ? "titleNoNumber" : "title",
                body: issueCount > 1 ? "bodyMany" : "bodyOne",
                values
            )
        }
    }

    public enum MergeRunPr {
        public static let entry = primary(
            "merge-run-pr",
            texts: [
                "title": "Merge PR #{number}?",
                "titleNoNumber": "Merge this pull request?",
                "body": "It is squash-merged. No issue is linked to it.",
            ],
            params: ["number"], answer: "merge", label: "Merge"
        )
        public static func copy(number: Int?) -> PromptCopy {
            guard let number else { return entry.copy(title: "titleNoNumber") }
            return entry.copy(["number": "\(number)"])
        }
    }

    /// EXP-1244: an open pull request NO issue or run links (Reviews
    /// repository bands), merged through `repositories.mergePull`.
    public enum MergeExternalPr {
        public static let entry = primary(
            "merge-external-pr",
            texts: [
                "title": "Merge {repository}#{number}?",
                "body": "It is squash-merged into {base}. No issue is linked to it.",
            ],
            params: ["repository", "number", "base"], answer: "merge", label: "Merge"
        )
        public static func copy(repository: String, number: Int, base: String) -> PromptCopy {
            entry.copy(["repository": repository, "number": "\(number)", "base": base])
        }
    }

    public enum StopRun {
        public static let entry = destructive(
            "stop-run",
            texts: ["title": "Stop this run?"],
            answer: "stop", label: "Stop"
        )
        public static func copy() -> PromptCopy { entry.copy() }
    }

    public enum ResumeRun {
        public static let entry = primary(
            "resume-run",
            texts: [
                "title": "Resume this run on {device}?",
                "titleNoDevice": "Resume this run?",
                "body": "The agent continues where it stopped, in the same worktree.",
            ],
            params: ["device"], answer: "resume", label: "Resume"
        )
        /// `device` = the label of the device that ran it; nil or empty =
        /// no known label.
        public static func copy(device: String?) -> PromptCopy {
            guard let device, !device.isEmpty else { return entry.copy(title: "titleNoDevice") }
            return entry.copy(["device": device])
        }
    }

    public enum DeleteTrigger {
        public static let entry = destructive(
            "delete-trigger",
            texts: [
                "title": "Delete this trigger?",
                "body": "Past runs stay in Runs.",
            ],
            answer: "delete", label: "Delete"
        )
        public static func copy() -> PromptCopy { entry.copy() }
    }

    public enum DeleteAction {
        public static let entry = destructive(
            "delete-action",
            texts: [
                "title": "Delete \"{name}\"?",
                "body": "Its triggers are deleted with it. Its runs stay and live ones keep going.",
            ],
            params: ["name"], answer: "delete", label: "Delete"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum RemoveDevice {
        public static let entry = destructive(
            "remove-device",
            texts: [
                "title": "Remove \"{name}\"?",
                "body": "A device with the daemon still running registers again on its next heartbeat.",
            ],
            params: ["name"], answer: "remove", label: "Remove"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum DeleteTeam {
        public static let entry = destructive(
            "delete-team",
            texts: [
                "title": "Delete \"{name}\"?",
                "body": "All its boards, issues and files are deleted for good.",
            ],
            params: ["name"], answer: "delete", label: "Delete"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum TrashBoard {
        public static let entry = destructive(
            "trash-board",
            texts: [
                "title": "Move \"{name}\" to trash?",
                "body": "After 48 hours it is deleted with all its issues. Until then an owner can restore it in team settings on the web.",
            ],
            params: ["name"], answer: "trash", label: "Move to trash"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum DeleteLabel {
        public static let entry = destructive(
            "delete-label",
            texts: [
                "title": "Delete \"{name}\"?",
                "body": "It comes off every issue that has it.",
            ],
            params: ["name"], answer: "delete", label: "Delete"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum RemoveMember {
        public static let entry = destructive(
            "remove-member",
            texts: [
                "title": "Remove {name} from this team?",
                "body": "They lose access immediately.",
            ],
            params: ["name"], answer: "remove", label: "Remove"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum LeaveTeam {
        public static let entry = destructive(
            "leave-team",
            texts: [
                "title": "Leave \"{name}\"?",
                "body": "You need a new invite to come back.",
            ],
            params: ["name"], answer: "leave", label: "Leave"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum MakeOwner {
        public static let entry = primary(
            "make-owner",
            texts: [
                "title": "Make {name} an owner?",
                "body": "Owners can manage members and billing, delete boards and delete the team.",
            ],
            params: ["name"], answer: "make-owner", label: "Make owner"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum MakeMember {
        public static let entry = primary(
            "make-member",
            texts: [
                "title": "Make {name} a member?",
                "body": "They can no longer manage members or delete boards.",
            ],
            params: ["name"], answer: "make-member", label: "Make member"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum RemoveRepository {
        public static let entry = destructive(
            "remove-repository",
            texts: ["title": "Remove {fullName} from this team?"],
            params: ["fullName"], answer: "remove", label: "Remove"
        )
        public static func copy(fullName: String) -> PromptCopy {
            entry.copy(["fullName": fullName])
        }
    }

    public enum UnlinkSignInMethod {
        public static let entry = destructive(
            "unlink-sign-in-method",
            texts: [
                "title": "Unlink {provider}?",
                "body": "Your other sign-in methods keep working. You can link it again any time.",
            ],
            params: ["provider"], answer: "unlink", label: "Unlink"
        )
        public static func copy(provider: String) -> PromptCopy {
            entry.copy(["provider": provider])
        }
    }

    public enum RemovePassword {
        public static let entry = destructive(
            "remove-password",
            texts: [
                "title": "Remove your password?",
                "body": "Your other sign-in methods keep working.",
            ],
            answer: "remove", label: "Remove"
        )
        public static func copy() -> PromptCopy { entry.copy() }
    }

    public enum RemovePasskey {
        public static let entry = destructive(
            "remove-passkey",
            texts: [
                "title": "Remove the passkey \"{name}\"?",
                "body": "The copy on your device stays until you delete it there.",
            ],
            params: ["name"], answer: "remove", label: "Remove"
        )
        public static func copy(name: String) -> PromptCopy {
            entry.copy(["name": name])
        }
    }

    public enum DeleteAccount {
        public static let entry = destructive(
            "delete-account",
            texts: [
                "title": "Delete your account?",
                "titleOnServer": "Delete your account on {server}?",
                "body": "Teams where you are the only member are deleted with all their issues. In shared teams your issues stay and your comments are deleted.",
            ],
            params: ["server"], answer: "delete", label: "Delete"
        )
        /// iOS holds several servers: the title names the one (`titleOnServer`)
        /// whenever its name is known.
        public static func copy(server: String?) -> PromptCopy {
            guard let server, !server.isEmpty else { return entry.copy() }
            return entry.copy(title: "titleOnServer", ["server": server])
        }
    }

    public enum RemoveServer {
        public static let entry = destructive(
            "remove-server",
            texts: [
                "title": "Remove {server}?",
                "body": "You are signed out and its data on this device is deleted. You can add it again any time.",
            ],
            params: ["server"], answer: "remove", label: "Remove"
        )
        public static func copy(server: String) -> PromptCopy {
            entry.copy(["server": server])
        }
    }

    /// VAPP-91: a host asking before a surface acts as the person. Deny
    /// takes focus and Return; Allow is the explicit `default` answer.
    public enum ExponentialUiConsent {
        public static let entry = PromptEntry(
            id: "exponential-ui-consent",
            texts: [
                "title": "Allow this surface to run {tool}?",
                "body": "It acts as you, with your access to this team.",
            ],
            params: ["tool"],
            actions: [
                PromptAction(id: "deny", label: "Deny", role: .cancel),
                PromptAction(id: "allow", label: "Allow", role: .default),
            ],
            focus: "deny"
        )
        public static func copy(tool: String) -> PromptCopy {
            entry.copy(["tool": tool])
        }
    }

    /// Every mirrored entry, locked against the fixture's `prompts` key set.
    public static let all: [PromptEntry] = [
        DeleteIssue.entry, DeleteIssues.entry, DeleteFile.entry, MoveIssue.entry,
        MergeIssuePr.entry, MergeRunPr.entry, MergeExternalPr.entry, StopRun.entry, ResumeRun.entry,
        DeleteTrigger.entry, DeleteAction.entry, RemoveDevice.entry, DeleteTeam.entry, TrashBoard.entry,
        DeleteLabel.entry, RemoveMember.entry, LeaveTeam.entry, MakeOwner.entry,
        MakeMember.entry, RemoveRepository.entry, UnlinkSignInMethod.entry,
        RemovePassword.entry, RemovePasskey.entry, DeleteAccount.entry,
        RemoveServer.entry, ExponentialUiConsent.entry,
    ]
}
