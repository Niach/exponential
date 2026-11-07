import Foundation

// VAPP-120: typed host ACTIONS, the iOS mirror of the TS reference
// `packages/domain-contract/src/app-actions.ts` (fixtures/app-actions.json is
// the spec; web `app-actions.ts`, desktop `domain::app_actions`, Android
// `AppActions.kt` replay the same cases): which `exponential_<noun>_<op>`
// functions exist, what a name resolves to, what a package declares, what an
// app's grants allow, and the consent card's words.
public enum AppActions {
    public static let prefix = "exponential_"

    /// Every function name, sorted (the fixture's `functions`).
    public static let all: [String] = [
        "exponential_agents_get", "exponential_agents_list",
        "exponential_apps_catalog", "exponential_apps_delete", "exponential_apps_get", "exponential_apps_handoff", "exponential_apps_list", "exponential_apps_records_delete", "exponential_apps_records_list", "exponential_apps_records_upsert", "exponential_apps_upsert",
        "exponential_attachments_delete", "exponential_attachments_get", "exponential_attachments_list", "exponential_attachments_upload",
        "exponential_boards_create", "exponential_boards_delete", "exponential_boards_get", "exponential_boards_list", "exponential_boards_set_repository", "exponential_boards_update",
        "exponential_comments_create", "exponential_comments_delete", "exponential_comments_list", "exponential_comments_update",
        "exponential_devices_account_login", "exponential_devices_list",
        "exponential_events_emit", "exponential_events_list",
        "exponential_issues_activity", "exponential_issues_create", "exponential_issues_delete", "exponential_issues_get", "exponential_issues_labels_add", "exponential_issues_labels_remove", "exponential_issues_list", "exponential_issues_relations_add", "exponential_issues_relations_remove", "exponential_issues_show", "exponential_issues_subscribe", "exponential_issues_unsubscribe", "exponential_issues_update", "exponential_issues_update_status",
        "exponential_labels_create", "exponential_labels_delete", "exponential_labels_get", "exponential_labels_list", "exponential_labels_update",
        "exponential_mcp_servers_create", "exponential_mcp_servers_delete", "exponential_mcp_servers_list",
        "exponential_notifications_list", "exponential_notifications_mark_read",
        "exponential_people_add", "exponential_people_invites_create", "exponential_people_invites_list", "exponential_people_invites_revoke", "exponential_people_list",
        "exponential_pr_close", "exponential_pr_files", "exponential_pr_merge", "exponential_pr_open", "exponential_pr_retarget", "exponential_pr_unlink", "exponential_pr_update",
        "exponential_prompts_create", "exponential_prompts_delete", "exponential_prompts_get", "exponential_prompts_list", "exponential_prompts_update",
        "exponential_repositories_create", "exponential_repositories_list",
        "exponential_sessions_end", "exponential_sessions_get", "exponential_sessions_link", "exponential_sessions_list",
        "exponential_sessions_send",
        "exponential_sessions_unlink",
        "exponential_statuses_create", "exponential_statuses_delete", "exponential_statuses_list", "exponential_statuses_update",
        "exponential_teams_create", "exponential_teams_get", "exponential_teams_list", "exponential_teams_update",
    ]

    /// The fixture's `tools`: what each function runs. An `exponential_*` name
    /// parses ONLY through this table, never by splitting on `_` (round 9 §6.3).
    private static let tools: [String: ActionTarget] = [
        "exponential_agents_get": ActionTarget(tool: "exponential_agents", op: "get"),
        "exponential_agents_list": ActionTarget(tool: "exponential_agents", op: "list"),
        "exponential_apps_catalog": ActionTarget(tool: "exponential_apps", op: "catalog"),
        "exponential_apps_delete": ActionTarget(tool: "exponential_apps", op: "delete"),
        "exponential_apps_get": ActionTarget(tool: "exponential_apps", op: "get"),
        "exponential_apps_handoff": ActionTarget(tool: "exponential_apps", op: "handoff"),
        "exponential_apps_list": ActionTarget(tool: "exponential_apps", op: "list"),
        "exponential_apps_records_delete": ActionTarget(tool: "exponential_apps", op: "records_delete"),
        "exponential_apps_records_list": ActionTarget(tool: "exponential_apps", op: "records_list"),
        "exponential_apps_records_upsert": ActionTarget(tool: "exponential_apps", op: "records_upsert"),
        "exponential_apps_upsert": ActionTarget(tool: "exponential_apps", op: "upsert"),
        "exponential_attachments_delete": ActionTarget(tool: "exponential_attachments", op: "delete"),
        "exponential_attachments_get": ActionTarget(tool: "exponential_attachments", op: "get"),
        "exponential_attachments_list": ActionTarget(tool: "exponential_attachments", op: "list"),
        "exponential_attachments_upload": ActionTarget(tool: "exponential_attachments", op: "upload"),
        "exponential_boards_create": ActionTarget(tool: "exponential_boards", op: "create"),
        "exponential_boards_delete": ActionTarget(tool: "exponential_boards", op: "delete"),
        "exponential_boards_get": ActionTarget(tool: "exponential_boards", op: "get"),
        "exponential_boards_list": ActionTarget(tool: "exponential_boards", op: "list"),
        "exponential_boards_set_repository": ActionTarget(tool: "exponential_boards", op: "set_repository"),
        "exponential_boards_update": ActionTarget(tool: "exponential_boards", op: "update"),
        "exponential_comments_create": ActionTarget(tool: "exponential_comments", op: "create"),
        "exponential_comments_delete": ActionTarget(tool: "exponential_comments", op: "delete"),
        "exponential_comments_list": ActionTarget(tool: "exponential_comments", op: "list"),
        "exponential_comments_update": ActionTarget(tool: "exponential_comments", op: "update"),
        "exponential_devices_account_login": ActionTarget(tool: "exponential_devices", op: "account_login"),
        "exponential_devices_list": ActionTarget(tool: "exponential_devices", op: "list"),
        "exponential_events_emit": ActionTarget(tool: "exponential_events", op: "emit"),
        "exponential_events_list": ActionTarget(tool: "exponential_events", op: "list"),
        "exponential_issues_activity": ActionTarget(tool: "exponential_issues", op: "activity"),
        "exponential_issues_create": ActionTarget(tool: "exponential_issues", op: "create"),
        "exponential_issues_delete": ActionTarget(tool: "exponential_issues", op: "delete"),
        "exponential_issues_get": ActionTarget(tool: "exponential_issues", op: "get"),
        "exponential_issues_labels_add": ActionTarget(tool: "exponential_issues", op: "labels_add"),
        "exponential_issues_labels_remove": ActionTarget(tool: "exponential_issues", op: "labels_remove"),
        "exponential_issues_list": ActionTarget(tool: "exponential_issues", op: "list"),
        "exponential_issues_relations_add": ActionTarget(tool: "exponential_issues", op: "relations_add"),
        "exponential_issues_relations_remove": ActionTarget(tool: "exponential_issues", op: "relations_remove"),
        "exponential_issues_show": ActionTarget(tool: "exponential_issues", op: "show"),
        "exponential_issues_subscribe": ActionTarget(tool: "exponential_issues", op: "subscribe"),
        "exponential_issues_unsubscribe": ActionTarget(tool: "exponential_issues", op: "unsubscribe"),
        "exponential_issues_update": ActionTarget(tool: "exponential_issues", op: "update"),
        "exponential_issues_update_status": ActionTarget(tool: "exponential_issues", op: "update_status"),
        "exponential_labels_create": ActionTarget(tool: "exponential_labels", op: "create"),
        "exponential_labels_delete": ActionTarget(tool: "exponential_labels", op: "delete"),
        "exponential_labels_get": ActionTarget(tool: "exponential_labels", op: "get"),
        "exponential_labels_list": ActionTarget(tool: "exponential_labels", op: "list"),
        "exponential_labels_update": ActionTarget(tool: "exponential_labels", op: "update"),
        "exponential_mcp_servers_create": ActionTarget(tool: "exponential_mcp_servers", op: "create"),
        "exponential_mcp_servers_delete": ActionTarget(tool: "exponential_mcp_servers", op: "delete"),
        "exponential_mcp_servers_list": ActionTarget(tool: "exponential_mcp_servers", op: "list"),
        "exponential_notifications_list": ActionTarget(tool: "exponential_notifications", op: "list"),
        "exponential_notifications_mark_read": ActionTarget(tool: "exponential_notifications", op: "mark_read"),
        "exponential_people_add": ActionTarget(tool: "exponential_people", op: "add"),
        "exponential_people_invites_create": ActionTarget(tool: "exponential_people", op: "invites_create"),
        "exponential_people_invites_list": ActionTarget(tool: "exponential_people", op: "invites_list"),
        "exponential_people_invites_revoke": ActionTarget(tool: "exponential_people", op: "invites_revoke"),
        "exponential_people_list": ActionTarget(tool: "exponential_people", op: "list"),
        "exponential_pr_close": ActionTarget(tool: "exponential_pr", op: "close"),
        "exponential_pr_files": ActionTarget(tool: "exponential_pr", op: "files"),
        "exponential_pr_merge": ActionTarget(tool: "exponential_pr", op: "merge"),
        "exponential_pr_open": ActionTarget(tool: "exponential_pr", op: "open"),
        "exponential_pr_retarget": ActionTarget(tool: "exponential_pr", op: "retarget"),
        "exponential_pr_unlink": ActionTarget(tool: "exponential_pr", op: "unlink"),
        "exponential_pr_update": ActionTarget(tool: "exponential_pr", op: "update"),
        "exponential_prompts_create": ActionTarget(tool: "exponential_prompts", op: "create"),
        "exponential_prompts_delete": ActionTarget(tool: "exponential_prompts", op: "delete"),
        "exponential_prompts_get": ActionTarget(tool: "exponential_prompts", op: "get"),
        "exponential_prompts_list": ActionTarget(tool: "exponential_prompts", op: "list"),
        "exponential_prompts_update": ActionTarget(tool: "exponential_prompts", op: "update"),
        "exponential_repositories_create": ActionTarget(tool: "exponential_repositories", op: "create"),
        "exponential_repositories_list": ActionTarget(tool: "exponential_repositories", op: "list"),
        "exponential_sessions_end": ActionTarget(tool: "exponential_sessions", op: "end"),
        "exponential_sessions_get": ActionTarget(tool: "exponential_sessions", op: "get"),
        "exponential_sessions_link": ActionTarget(tool: "exponential_sessions", op: "link"),
        "exponential_sessions_list": ActionTarget(tool: "exponential_sessions", op: "list"),
        "exponential_sessions_send": ActionTarget(tool: "exponential_sessions_send", op: nil),
        "exponential_sessions_unlink": ActionTarget(tool: "exponential_sessions", op: "unlink"),
        "exponential_statuses_create": ActionTarget(tool: "exponential_statuses", op: "create"),
        "exponential_statuses_delete": ActionTarget(tool: "exponential_statuses", op: "delete"),
        "exponential_statuses_list": ActionTarget(tool: "exponential_statuses", op: "list"),
        "exponential_statuses_update": ActionTarget(tool: "exponential_statuses", op: "update"),
        "exponential_teams_create": ActionTarget(tool: "exponential_teams", op: "create"),
        "exponential_teams_get": ActionTarget(tool: "exponential_teams", op: "get"),
        "exponential_teams_list": ActionTarget(tool: "exponential_teams", op: "list"),
        "exponential_teams_update": ActionTarget(tool: "exponential_teams", op: "update"),
    ]

    /// EXP-1657: the names from before the `team_config` split (old → new;
    /// the fixture's `aliases`). A host registers each as a function too, so
    /// a published package keeps working; never listed in `all` or a
    /// declared list.
    public static let aliases: [String: String] = [
        "exponential_team_config_agents_list": "exponential_agents_list",
        "exponential_team_config_labels_create": "exponential_labels_create",
        "exponential_team_config_labels_delete": "exponential_labels_delete",
        "exponential_team_config_labels_get": "exponential_labels_get",
        "exponential_team_config_labels_list": "exponential_labels_list",
        "exponential_team_config_labels_update": "exponential_labels_update",
        "exponential_team_config_mcp_servers_add": "exponential_mcp_servers_create",
        "exponential_team_config_mcp_servers_list": "exponential_mcp_servers_list",
        "exponential_team_config_mcp_servers_remove": "exponential_mcp_servers_delete",
        "exponential_team_config_prompts_create": "exponential_prompts_create",
        "exponential_team_config_prompts_delete": "exponential_prompts_delete",
        "exponential_team_config_prompts_get": "exponential_prompts_get",
        "exponential_team_config_prompts_list": "exponential_prompts_list",
        "exponential_team_config_prompts_update": "exponential_prompts_update",
        "exponential_team_config_repositories_add": "exponential_repositories_create",
        "exponential_team_config_repositories_list": "exponential_repositories_list",
        "exponential_team_config_statuses_create": "exponential_statuses_create",
        "exponential_team_config_statuses_delete": "exponential_statuses_delete",
        "exponential_team_config_statuses_list": "exponential_statuses_list",
        "exponential_team_config_statuses_update": "exponential_statuses_update",
    ]

    /// The name an action is judged by: an alias's new name, else the name.
    public static func canonicalAction(_ name: String) -> String {
        aliases[name] ?? name
    }

    /// The `copy.tools` strings (the editor's Settings Tools group; VAPP-177:
    /// phones show it in the `…` menu's Settings sheet).
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

    /// The fixture's `destructiveWords` (`AppActionsTests` locks the copy).
    static let destructiveWords: Set<String> = ["delete", "merge", "close", "end", "remove", "trash"]

    /// The fixture's `destructiveArgs`: an arg that makes a grantable
    /// action's call destructive (`exponential_sessions_send` + `stop` = stop-run).
    static let destructiveArgs: [String: [String]] = ["exponential_sessions_send": ["stop"]]

    /// A destructive CALL: a destructive action, or a grantable one whose
    /// args carry a `destructiveArgs` key (present and not false/null).
    public static func callDestructive(_ name: String, args: [String: CardJSON]) -> Bool {
        if actionDestructive(name) { return true }
        return (destructiveArgs[canonicalAction(name)] ?? []).contains { key in
            guard let value = args[key] else { return false }
            return value != .null && value != .bool(false)
        }
    }

    /// A destructive action: a word of its tool name or op is delete, merge,
    /// close, end, remove or trash. Never grantable; always asks.
    public static func actionDestructive(_ name: String) -> Bool {
        guard let target = parseActionName(name) else { return false }
        let words = "\(target.tool) \(target.op ?? "")".lowercased().split(whereSeparator: { !$0.isLetter })
        return words.contains { destructiveWords.contains(String($0)) }
    }

    /// VAPP-158: the fixture's `ownerActions` (`AppActionsTests` locks the
    /// list): the non-destructive operations the server runs for a team
    /// owner only (members, invites, MCP servers, prompts, team settings).
    static let ownerActions: Set<String> = [
        "exponential_people_add", "exponential_people_invites_create", "exponential_people_invites_revoke",
        "exponential_mcp_servers_create", "exponential_prompts_create", "exponential_prompts_update",
        "exponential_teams_update",
    ]

    /// An owner-only operation: never grantable; always asks (the plain
    /// card, `callDestructive` is unchanged).
    public static func actionOwnerOnly(_ name: String) -> Bool {
        ownerActions.contains(canonicalAction(name))
    }

    /// May `name` be granted at all: an offered action that is neither
    /// destructive nor owner-only.
    public static func actionGrantable(_ name: String) -> Bool {
        parseActionName(name) != nil && !actionDestructive(name) && !actionOwnerOnly(name)
    }

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

    /// `exponential_issues_update_status` → the tool and op it runs (an
    /// alias → its new name's); nil for anything that is not an offered
    /// action.
    public static func parseActionName(_ name: String) -> ActionTarget? {
        tools[canonicalAction(name)]
    }

    /// VAPP-107's pattern rule: an exact name or a `prefix*`.
    private static func matches(_ pattern: String, _ name: String) -> Bool {
        pattern.hasSuffix("*") ? name.hasPrefix(String(pattern.dropLast())) : pattern == name
    }

    /// The actions a package's `functions` list names (patterns expanded,
    /// over the aliases too: an alias declares its NEW name), in the
    /// fixture's order; anything that is not an offered action is dropped.
    public static func declaredActions(_ functions: [String]) -> [String] {
        let named = { (name: String) in functions.contains { matches($0, name) } }
        let viaAlias = Set(aliases.filter { named($0.key) }.values)
        return all.filter { named($0) || viaAlias.contains($0) }
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

    /// Does the app's `granted_tools` list let this call run without
    /// asking? Exact names only (a grant is a person's choice, never a
    /// pattern; an alias and its new name are the same action), and a
    /// destructive action or call ignores a stored grant.
    public static func grantAllows(_ granted: [String], _ name: String, args: [String: CardJSON]) -> Bool {
        let action = canonicalAction(name)
        return actionGrantable(name) && !callDestructive(name, args: args) && granted.contains { canonicalAction($0) == action }
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
            destructive: callDestructive(name, args: Dictionary(args.map { ($0.key, $0.value) }, uniquingKeysWith: { _, last in last })),
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
