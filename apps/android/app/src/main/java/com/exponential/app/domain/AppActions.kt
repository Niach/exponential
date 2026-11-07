package com.exponential.app.domain

import at.exponential.ui.json.JsonValue

// VAPP-120: typed host ACTIONS, the Android mirror of the pure reference
// `packages/domain-contract/src/app-actions.ts` (fixtures/app-actions.json is
// the spec; `AppActionsTest` replays its `parseCases`, `grantCases`,
// `declaredCases`, `consentCases` and `ownerOnlyCases` and locks [ALL], [TOOLS] and [Copy] to
// the fixture): which `exponential_<noun>_<op>` functions exist, what a name
// resolves to, what a package declares, what an app's grants allow, and the
// consent card's words. The display titles come from the generated contract
// (`DomainContract.expTool*`).
object AppActions {
    const val PREFIX = "exponential_"

    /** The MCP tool (`exponential_issues`) and the noun's op (null for a verb tool). */
    data class Target(val tool: String, val op: String?)

    /** Every function name → what it runs, in the fixture's (sorted) order. */
    val TOOLS: Map<String, Target> = linkedMapOf(
        "exponential_agents_get" to Target("exponential_agents", "get"),
        "exponential_agents_list" to Target("exponential_agents", "list"),
        "exponential_apps_catalog" to Target("exponential_apps", "catalog"),
        "exponential_apps_delete" to Target("exponential_apps", "delete"),
        "exponential_apps_get" to Target("exponential_apps", "get"),
        "exponential_apps_handoff" to Target("exponential_apps", "handoff"),
        "exponential_apps_list" to Target("exponential_apps", "list"),
        "exponential_apps_records_delete" to Target("exponential_apps", "records_delete"),
        "exponential_apps_records_list" to Target("exponential_apps", "records_list"),
        "exponential_apps_records_upsert" to Target("exponential_apps", "records_upsert"),
        "exponential_apps_upsert" to Target("exponential_apps", "upsert"),
        "exponential_attachments_delete" to Target("exponential_attachments", "delete"),
        "exponential_attachments_get" to Target("exponential_attachments", "get"),
        "exponential_attachments_list" to Target("exponential_attachments", "list"),
        "exponential_attachments_upload" to Target("exponential_attachments", "upload"),
        "exponential_boards_create" to Target("exponential_boards", "create"),
        "exponential_boards_delete" to Target("exponential_boards", "delete"),
        "exponential_boards_get" to Target("exponential_boards", "get"),
        "exponential_boards_list" to Target("exponential_boards", "list"),
        "exponential_boards_set_repository" to Target("exponential_boards", "set_repository"),
        "exponential_boards_update" to Target("exponential_boards", "update"),
        "exponential_comments_create" to Target("exponential_comments", "create"),
        "exponential_comments_delete" to Target("exponential_comments", "delete"),
        "exponential_comments_list" to Target("exponential_comments", "list"),
        "exponential_comments_update" to Target("exponential_comments", "update"),
        "exponential_devices_account_login" to Target("exponential_devices", "account_login"),
        "exponential_devices_list" to Target("exponential_devices", "list"),
        "exponential_events_emit" to Target("exponential_events", "emit"),
        "exponential_events_list" to Target("exponential_events", "list"),
        "exponential_issues_activity" to Target("exponential_issues", "activity"),
        "exponential_issues_create" to Target("exponential_issues", "create"),
        "exponential_issues_delete" to Target("exponential_issues", "delete"),
        "exponential_issues_get" to Target("exponential_issues", "get"),
        "exponential_issues_labels_add" to Target("exponential_issues", "labels_add"),
        "exponential_issues_labels_remove" to Target("exponential_issues", "labels_remove"),
        "exponential_issues_list" to Target("exponential_issues", "list"),
        "exponential_issues_relations_add" to Target("exponential_issues", "relations_add"),
        "exponential_issues_relations_remove" to Target("exponential_issues", "relations_remove"),
        "exponential_issues_show" to Target("exponential_issues", "show"),
        "exponential_issues_subscribe" to Target("exponential_issues", "subscribe"),
        "exponential_issues_unsubscribe" to Target("exponential_issues", "unsubscribe"),
        "exponential_issues_update" to Target("exponential_issues", "update"),
        "exponential_issues_update_status" to Target("exponential_issues", "update_status"),
        "exponential_labels_create" to Target("exponential_labels", "create"),
        "exponential_labels_delete" to Target("exponential_labels", "delete"),
        "exponential_labels_get" to Target("exponential_labels", "get"),
        "exponential_labels_list" to Target("exponential_labels", "list"),
        "exponential_labels_update" to Target("exponential_labels", "update"),
        "exponential_mcp_servers_create" to Target("exponential_mcp_servers", "create"),
        "exponential_mcp_servers_delete" to Target("exponential_mcp_servers", "delete"),
        "exponential_mcp_servers_list" to Target("exponential_mcp_servers", "list"),
        "exponential_notifications_list" to Target("exponential_notifications", "list"),
        "exponential_notifications_mark_read" to Target("exponential_notifications", "mark_read"),
        "exponential_people_add" to Target("exponential_people", "add"),
        "exponential_people_invites_create" to Target("exponential_people", "invites_create"),
        "exponential_people_invites_list" to Target("exponential_people", "invites_list"),
        "exponential_people_invites_revoke" to Target("exponential_people", "invites_revoke"),
        "exponential_people_list" to Target("exponential_people", "list"),
        "exponential_pr_close" to Target("exponential_pr", "close"),
        "exponential_pr_files" to Target("exponential_pr", "files"),
        "exponential_pr_merge" to Target("exponential_pr", "merge"),
        "exponential_pr_open" to Target("exponential_pr", "open"),
        "exponential_pr_retarget" to Target("exponential_pr", "retarget"),
        "exponential_pr_unlink" to Target("exponential_pr", "unlink"),
        "exponential_pr_update" to Target("exponential_pr", "update"),
        "exponential_prompts_create" to Target("exponential_prompts", "create"),
        "exponential_prompts_delete" to Target("exponential_prompts", "delete"),
        "exponential_prompts_get" to Target("exponential_prompts", "get"),
        "exponential_prompts_list" to Target("exponential_prompts", "list"),
        "exponential_prompts_update" to Target("exponential_prompts", "update"),
        "exponential_repositories_create" to Target("exponential_repositories", "create"),
        "exponential_repositories_list" to Target("exponential_repositories", "list"),
        "exponential_sessions_end" to Target("exponential_sessions", "end"),
        "exponential_sessions_get" to Target("exponential_sessions", "get"),
        "exponential_sessions_link" to Target("exponential_sessions", "link"),
        "exponential_sessions_list" to Target("exponential_sessions", "list"),
        "exponential_sessions_send" to Target("exponential_sessions_send", null),
        "exponential_sessions_unlink" to Target("exponential_sessions", "unlink"),
        "exponential_statuses_create" to Target("exponential_statuses", "create"),
        "exponential_statuses_delete" to Target("exponential_statuses", "delete"),
        "exponential_statuses_list" to Target("exponential_statuses", "list"),
        "exponential_statuses_update" to Target("exponential_statuses", "update"),
        "exponential_teams_create" to Target("exponential_teams", "create"),
        "exponential_teams_get" to Target("exponential_teams", "get"),
        "exponential_teams_list" to Target("exponential_teams", "list"),
        "exponential_teams_update" to Target("exponential_teams", "update"),
    )

    /** Every function name, sorted. */
    val ALL: List<String> = TOOLS.keys.toList()

    /** EXP-1657: the names from before the `team_config` split (old → new;
     *  the fixture's `aliases`). A host registers each as a function too, so
     *  a published package keeps working; never in [ALL] or a declared list. */
    val ALIASES: Map<String, String> = linkedMapOf(
        "exponential_team_config_agents_list" to "exponential_agents_list",
        "exponential_team_config_labels_create" to "exponential_labels_create",
        "exponential_team_config_labels_delete" to "exponential_labels_delete",
        "exponential_team_config_labels_get" to "exponential_labels_get",
        "exponential_team_config_labels_list" to "exponential_labels_list",
        "exponential_team_config_labels_update" to "exponential_labels_update",
        "exponential_team_config_mcp_servers_add" to "exponential_mcp_servers_create",
        "exponential_team_config_mcp_servers_list" to "exponential_mcp_servers_list",
        "exponential_team_config_mcp_servers_remove" to "exponential_mcp_servers_delete",
        "exponential_team_config_prompts_create" to "exponential_prompts_create",
        "exponential_team_config_prompts_delete" to "exponential_prompts_delete",
        "exponential_team_config_prompts_get" to "exponential_prompts_get",
        "exponential_team_config_prompts_list" to "exponential_prompts_list",
        "exponential_team_config_prompts_update" to "exponential_prompts_update",
        "exponential_team_config_repositories_add" to "exponential_repositories_create",
        "exponential_team_config_repositories_list" to "exponential_repositories_list",
        "exponential_team_config_statuses_create" to "exponential_statuses_create",
        "exponential_team_config_statuses_delete" to "exponential_statuses_delete",
        "exponential_team_config_statuses_list" to "exponential_statuses_list",
        "exponential_team_config_statuses_update" to "exponential_statuses_update",
    )

    /** The name an action is judged by: an alias's new name, else the name. */
    fun canonicalAction(name: String): String = ALIASES[name] ?: name

    /** The settings page's `copy.tools` strings (web + desktop render them; phones have no app settings). */
    object Copy {
        const val TITLE = "Tools"
        const val HINT = "What this app may do as you without asking. Anything else asks each time."
        const val NONE = "This app uses no tools."
        const val GRANTED = "Allowed"
        /** The row of a destructive action, in place of its switch. */
        const val ALWAYS = "Always asks"
    }

    /** The team role that sets or changes an app's grants (the fixture's
     *  `grantRole`): the creator gets no grant power from creating it. */
    const val GRANT_ROLE = "owner"

    /** May a member with [role] set an app's grants? Owners only. */
    fun canGrantActions(role: String?): Boolean = role == GRANT_ROLE

    /** A destructive action: a word of its tool name or op is delete, merge,
     *  close, end, remove or trash. Never grantable; always asks. */
    fun actionDestructive(name: String): Boolean {
        val target = parseActionName(name) ?: return false
        val words = "${target.tool} ${target.op.orEmpty()}".lowercase().split(Regex("[^a-z]+"))
        return words.any { it in DESTRUCTIVE_WORDS }
    }

    /** VAPP-158: an operation the server runs for a team owner only (members,
     *  invites, MCP servers, prompts, the team's settings). Never grantable;
     *  always asks (the plain card). */
    fun actionOwnerOnly(name: String): Boolean = canonicalAction(name) in OWNER_ACTIONS

    /** May [name] be granted at all: an offered action that is neither
     *  destructive nor owner-only. */
    fun actionGrantable(name: String): Boolean =
        parseActionName(name) != null && !actionDestructive(name) && !actionOwnerOnly(name)

    /** `exponential_issues_update_status` → the tool and op it runs (an alias
     *  → its new name's); null for anything that is not an offered action. */
    fun parseActionName(name: String): Target? = TOOLS[canonicalAction(name)]

    /** VAPP-107's pattern rule: an exact name or a `prefix*`. */
    private fun matches(pattern: String, name: String): Boolean =
        if (pattern.endsWith("*")) name.startsWith(pattern.dropLast(1)) else pattern == name

    /** The actions a package's `functions` list names (patterns expanded, over
     *  the aliases too: an alias declares its NEW name), in the fixture's
     *  order; anything that is not an offered action is dropped. */
    fun declaredActions(functions: List<String>): List<String> {
        val named = { name: String -> functions.any { matches(it, name) } }
        val viaAlias = ALIASES.filterKeys(named).values.toSet()
        return ALL.filter { named(it) || it in viaAlias }
    }

    /** A typed action's MCP `arguments`: `{…args, op}` for a noun's op (the
     *  NAME's op wins: an app-supplied `op` key cannot change the operation the
     *  card and the grant name), the args alone for a verb tool. */
    fun arguments(target: Target, args: Map<String, JsonValue>): Map<String, JsonValue> =
        if (target.op != null) args + mapOf("op" to JsonValue.Str(target.op)) else args

    /** Does the app's `granted_tools` list let [name] run without asking?
     *  Exact names only (a grant is a person's choice, never a pattern; an
     *  alias and its new name are the same action), and a destructive or
     *  owner-only action ignores a stored grant: it always asks. */
    fun grantAllows(granted: List<String>, name: String, args: Map<String, JsonValue>): Boolean =
        actionGrantable(name) && !callDestructive(name, args) &&
            granted.any { canonicalAction(it) == canonicalAction(name) }

    /** An `apps.granted_tools` column (a JSON string[]) as a list; null,
     *  malformed or non-string entries read as none. */
    fun grantedList(json: String?): List<String> =
        json?.let { runCatching { JsonValue.parse(it) }.getOrNull() }?.array?.mapNotNull { it.string } ?: emptyList()

    /** The fixture's `destructiveWords` (`AppActionsTest` locks the copy). */
    internal val DESTRUCTIVE_WORDS = listOf("delete", "merge", "close", "end", "remove", "trash")

    /** The fixture's `ownerActions` (`AppActionsTest` locks the copy). */
    internal val OWNER_ACTIONS = listOf(
        "exponential_mcp_servers_create",
        "exponential_people_add",
        "exponential_people_invites_create",
        "exponential_people_invites_revoke",
        "exponential_prompts_create",
        "exponential_prompts_update",
        "exponential_teams_update",
    )

    /** The fixture's `destructiveArgs`: an arg that makes a grantable
     *  action's call destructive (`exponential_sessions_send` + `stop` = stop-run). */
    internal val DESTRUCTIVE_ARGS: Map<String, List<String>> = mapOf("exponential_sessions_send" to listOf("stop"))

    /** A destructive CALL: a destructive action, or a grantable one whose
     *  args carry a [DESTRUCTIVE_ARGS] key (present and not false/null). */
    fun callDestructive(name: String, args: Map<String, JsonValue>): Boolean =
        actionDestructive(name) || DESTRUCTIVE_ARGS[canonicalAction(name)].orEmpty().any { key ->
            val value = args[key]
            value != null && value != JsonValue.Null && value != JsonValue.Bool(false)
        }

    data class ActionConsent(
        val tool: String,
        val title: String,
        val destructive: Boolean,
        /** The arguments as compact JSON (key order kept), null when there are none. */
        val args: String?,
    )

    /** The contract display row's index for a target (`issues_update_status`), -1 when none. */
    private fun rowIndex(target: Target): Int {
        val row = target.tool.removePrefix(DomainContract.expToolPrefix) + (target.op?.let { "_$it" } ?: "")
        return DomainContract.expToolNames.indexOf(row)
    }

    /** The display row's title for an action (`Set issue status`), the name
     *  itself for anything else: the settings page's row label. */
    fun actionTitle(name: String): String {
        val target = parseActionName(name) ?: return name
        return DomainContract.expToolTitles.getOrNull(rowIndex(target)) ?: name
    }

    /** The consent card's words for a typed action call: the display row's
     *  title lower-cased after "Allow this surface to", the subject arg when
     *  the row names one, the destructive flag from the tool name and op, the
     *  args as compact JSON. Null for a name that is not an offered action
     *  (the generic `exponential-ui-consent` copy applies). */
    fun actionConsent(name: String, args: Map<String, JsonValue>): ActionConsent? {
        val target = parseActionName(name) ?: return null
        val at = rowIndex(target)
        val title = DomainContract.expToolTitles.getOrNull(at)
        val base = title?.let { it.take(1).lowercase() + it.drop(1) } ?: name
        val subjectKey = DomainContract.expToolSubjectKeys.getOrNull(at).orEmpty()
        val subject = if (title != null && subjectKey.isNotEmpty()) args[subjectKey]?.string else null
        val json = JsonValue.Obj(args).orderedJson
        return ActionConsent(
            tool = target.tool,
            title = "Allow this surface to $base${if (!subject.isNullOrEmpty()) " $subject" else ""}?",
            destructive = callDestructive(name, args),
            args = json.takeIf { it != "{}" },
        )
    }
}
