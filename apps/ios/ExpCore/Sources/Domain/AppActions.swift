import Foundation

// VAPP-120: typed host ACTIONS, the iOS mirror of the TS reference
// `packages/domain-contract/src/app-actions.ts` (fixtures/app-actions.json is
// the spec; web `app-actions.ts`, desktop `domain::app_actions`, Android
// `AppActions.kt` replay the same cases): which `exponential.<noun>.<op>`
// functions exist, what a name resolves to, what a package declares, what an
// app's grants allow, and the consent card's words.
public enum AppActions {
    public static let prefix = "exponential."

    /// Every function name, sorted (the fixture's `functions`).
    public static let all: [String] = [
        "exponential.apps.catalog", "exponential.apps.delete", "exponential.apps.get", "exponential.apps.list", "exponential.apps.records_delete", "exponential.apps.records_list", "exponential.apps.records_upsert", "exponential.apps.upsert",
        "exponential.attachments.delete", "exponential.attachments.get", "exponential.attachments.list", "exponential.attachments.upload",
        "exponential.boards.create", "exponential.boards.delete", "exponential.boards.get", "exponential.boards.list", "exponential.boards.set_repository", "exponential.boards.update",
        "exponential.comments.create", "exponential.comments.delete", "exponential.comments.list", "exponential.comments.update",
        "exponential.devices.account_login", "exponential.devices.list",
        "exponential.events.emit", "exponential.events.list",
        "exponential.issues.activity", "exponential.issues.create", "exponential.issues.delete", "exponential.issues.get", "exponential.issues.labels_add", "exponential.issues.labels_remove", "exponential.issues.list", "exponential.issues.relations_add", "exponential.issues.relations_remove", "exponential.issues.show", "exponential.issues.subscribe", "exponential.issues.unsubscribe", "exponential.issues.update", "exponential.issues.update_status",
        "exponential.notifications.list", "exponential.notifications.mark_read",
        "exponential.people.add", "exponential.people.invites_create", "exponential.people.invites_list", "exponential.people.invites_revoke", "exponential.people.list",
        "exponential.pr.close", "exponential.pr.files", "exponential.pr.merge", "exponential.pr.open", "exponential.pr.retarget", "exponential.pr.unlink", "exponential.pr.update",
        "exponential.sessions.end", "exponential.sessions.get", "exponential.sessions.link", "exponential.sessions.list", "exponential.sessions.send", "exponential.sessions.unlink",
        "exponential.team_config.agents_list", "exponential.team_config.labels_create", "exponential.team_config.labels_delete", "exponential.team_config.labels_get", "exponential.team_config.labels_list", "exponential.team_config.labels_update", "exponential.team_config.mcp_servers_add", "exponential.team_config.mcp_servers_list", "exponential.team_config.mcp_servers_remove", "exponential.team_config.prompts_get", "exponential.team_config.prompts_list", "exponential.team_config.repositories_add", "exponential.team_config.repositories_list", "exponential.team_config.statuses_create", "exponential.team_config.statuses_delete", "exponential.team_config.statuses_list", "exponential.team_config.statuses_update",
        "exponential.teams.create", "exponential.teams.get", "exponential.teams.list", "exponential.teams.update",
    ]

    /// The `copy.tools` strings (the settings page's Tools group; phones
    /// have no app settings, mirrored for parity).
    public enum Copy {
        public static let title = "Tools"
        public static let hint = "What this app may do as you without asking. Anything else asks each time."
        public static let none = "This app uses no tools."
        public static let granted = "Allowed"
        /// The row of a destructive action, in place of its switch.
        public static let always = "Always asks"
    }

    /// The team role that sets or changes an app's grants (the fixture's
    /// `grantRole`): the creator gets no grant power from creating it.
    public static let grantRole = "owner"

    /// May a member with `role` set an app's grants? Owners only.
    public static func canGrantActions(_ role: String?) -> Bool {
        role == grantRole
    }

    private static let destructiveWords: Set<String> = ["delete", "merge", "close", "end", "remove", "trash"]

    /// A destructive action: a word of its tool name or op is delete, merge,
    /// close, end, remove or trash. Never grantable; always asks.
    public static func actionDestructive(_ name: String) -> Bool {
        guard let target = parseActionName(name) else { return false }
        let words = "\(target.tool) \(target.op ?? "")".lowercased().split(whereSeparator: { !$0.isLetter })
        return words.contains { destructiveWords.contains(String($0)) }
    }

    /// May `name` be granted at all: an offered, non-destructive action.
    public static func actionGrantable(_ name: String) -> Bool {
        parseActionName(name) != nil && !actionDestructive(name)
    }

    /// The verb tools: a function that maps to a whole tool, no op.
    private static let verbTools: [String: String] = [
        "exponential.sessions.send": "exponential_sessions_send",
    ]

    private static let offered: Set<String> = Set(all)

    public struct ActionTarget: Equatable, Sendable {
        /// The MCP tool (`exponential_issues`).
        public let tool: String
        /// The noun's op, nil for a verb tool.
        public let op: String?

        public init(tool: String, op: String?) {
            self.tool = tool
            self.op = op
        }
    }

    /// `exponential.issues.update_status` → the tool and op it runs; nil for
    /// anything that is not an offered action.
    public static func parseActionName(_ name: String) -> ActionTarget? {
        guard offered.contains(name) else { return nil }
        if let tool = verbTools[name] { return ActionTarget(tool: tool, op: nil) }
        let parts = name.dropFirst(prefix.count).split(separator: ".", maxSplits: 1).map(String.init)
        guard parts.count == 2 else { return nil }
        return ActionTarget(tool: "exponential_\(parts[0])", op: parts[1])
    }

    /// VAPP-107's pattern rule: an exact name or a `prefix*`.
    private static func matches(_ pattern: String, _ name: String) -> Bool {
        pattern.hasSuffix("*") ? name.hasPrefix(String(pattern.dropLast())) : pattern == name
    }

    /// The actions a package's `functions` list names (patterns expanded),
    /// in the fixture's order; anything that is not an offered action is
    /// dropped.
    public static func declaredActions(_ functions: [String]) -> [String] {
        all.filter { name in functions.contains { matches($0, name) } }
    }

    /// A typed action's MCP `arguments`: `{…args, op}` for a noun's op (the
    /// NAME's op wins: an app-supplied `op` key cannot change the operation
    /// the card and the grant name), the args alone for a verb tool. Generic
    /// over the host's JSON value type (`string` makes the op's value).
    public static func arguments<Value>(_ args: [String: Value], op: String?, string: (String) -> Value) -> [String: Value] {
        guard let op else { return args }
        var merged = args
        merged["op"] = string(op)
        return merged
    }

    /// Does the app's `granted_tools` list let `name` run without asking?
    /// Exact names only (a grant is a person's choice, never a pattern), and
    /// a destructive action ignores a stored grant: it always asks.
    public static func grantAllows(_ granted: [String], _ name: String) -> Bool {
        actionGrantable(name) && granted.contains(name)
    }

    public struct ActionConsent: Equatable, Sendable {
        public let tool: String
        public let title: String
        public let destructive: Bool
        /// The arguments as compact JSON, nil when there are none.
        public let args: String?
    }

    private struct DisplayRow {
        let title: String
        let subjectKey: String
    }

    /// The contract's `expToolDisplay` row for a target (`issues_update_status`).
    private static func displayRow(_ target: ActionTarget) -> DisplayRow? {
        let tool = target.tool.hasPrefix("exponential_") ? String(target.tool.dropFirst("exponential_".count)) : target.tool
        let rowName = tool + (target.op.map { "_\($0)" } ?? "")
        guard let index = DomainContract.expToolNames.firstIndex(of: rowName),
              index < DomainContract.expToolTitles.count
        else { return nil }
        let subjectKeys = DomainContract.expToolSubjectKeys
        return DisplayRow(
            title: DomainContract.expToolTitles[index],
            subjectKey: index < subjectKeys.count ? subjectKeys[index] : ""
        )
    }

    /// The display row's title for an action (`Set issue status`), the name
    /// itself for anything else: the settings page's row label.
    public static func actionTitle(_ name: String) -> String {
        guard let target = parseActionName(name) else { return name }
        return displayRow(target)?.title ?? name
    }

    /// The consent card's words for a typed action call: the display row's
    /// title lower-cased after "Allow this surface to", the subject arg when
    /// the row names one, the destructive flag from the tool name and op, the
    /// args as compact JSON IN THE GIVEN KEY ORDER. Nil for a name that is
    /// not an offered action (the generic `exponential-ui-consent` copy
    /// applies).
    public static func actionConsent(_ name: String, args: [(key: String, value: CardJSON)]) -> ActionConsent? {
        guard let target = parseActionName(name) else { return nil }
        let row = displayRow(target)
        let base = row.map { $0.title.prefix(1).lowercased() + $0.title.dropFirst() } ?? name
        var subject = ""
        if let key = row?.subjectKey, !key.isEmpty,
           case let .string(value)? = args.first(where: { $0.key == key })?.value {
            subject = value
        }
        let title = "Allow this surface to \(base)\(subject.isEmpty ? "" : " \(subject)")?"
        let json = "{" + args.map { CardJSON.string($0.key).compactString + ":" + $0.value.compactString }.joined(separator: ",") + "}"
        return ActionConsent(
            tool: target.tool,
            title: title,
            destructive: actionDestructive(name),
            args: json == "{}" ? nil : json
        )
    }

    /// The unordered form (a host's args dictionary): keys sorted, the
    /// order every other iOS consent block prints.
    public static func actionConsent(_ name: String, args: [String: CardJSON]) -> ActionConsent? {
        actionConsent(name, args: args.keys.sorted().map { (key: $0, value: args[$0] ?? .null) })
    }
}

public extension Prompts {
    /// VAPP-120: the consent card's copy with a typed action's title in place
    /// of the generic prompt's (`AppActions.actionConsent`); nil = the plain
    /// `exponential-ui-consent` entry (plain or destructive) as is.
    static func exponentialUiConsent(tool: String, destructive: Bool, title: String?) -> PromptCopy {
        let generic = exponentialUiConsent(tool: tool, destructive: destructive)
        guard let title else { return generic }
        return PromptCopy(title: title, body: generic.body, actions: generic.actions, focus: generic.focus)
    }
}
