package com.exponential.app.domain

import at.exponential.ui.json.JsonValue

// VAPP-120: typed host ACTIONS, the Android mirror of the pure reference
// `packages/domain-contract/src/app-actions.ts` (fixtures/app-actions.json is
// the spec; `AppActionsTest` replays its `parseCases`, `grantCases`,
// `declaredCases` and `consentCases` and locks [ALL], [TOOLS] and [Copy] to
// the fixture): which `exponential.<noun>.<op>` functions exist, what a name
// resolves to, what a package declares, what an app's grants allow, and the
// consent card's words. The display titles come from the generated contract
// (`DomainContract.expTool*`).
object AppActions {
    const val PREFIX = "exponential."

    /** The MCP tool (`exponential_issues`) and the noun's op (null for a verb tool). */
    data class Target(val tool: String, val op: String?)

    /** Every function name → what it runs, in the fixture's (sorted) order. */
    val TOOLS: Map<String, Target> = linkedMapOf(
        "exponential.apps.catalog" to Target("exponential_apps", "catalog"),
        "exponential.apps.delete" to Target("exponential_apps", "delete"),
        "exponential.apps.get" to Target("exponential_apps", "get"),
        "exponential.apps.list" to Target("exponential_apps", "list"),
        "exponential.apps.records_delete" to Target("exponential_apps", "records_delete"),
        "exponential.apps.records_list" to Target("exponential_apps", "records_list"),
        "exponential.apps.records_upsert" to Target("exponential_apps", "records_upsert"),
        "exponential.apps.upsert" to Target("exponential_apps", "upsert"),
        "exponential.attachments.delete" to Target("exponential_attachments", "delete"),
        "exponential.attachments.get" to Target("exponential_attachments", "get"),
        "exponential.attachments.list" to Target("exponential_attachments", "list"),
        "exponential.attachments.upload" to Target("exponential_attachments", "upload"),
        "exponential.boards.create" to Target("exponential_boards", "create"),
        "exponential.boards.delete" to Target("exponential_boards", "delete"),
        "exponential.boards.get" to Target("exponential_boards", "get"),
        "exponential.boards.list" to Target("exponential_boards", "list"),
        "exponential.boards.set_repository" to Target("exponential_boards", "set_repository"),
        "exponential.boards.update" to Target("exponential_boards", "update"),
        "exponential.comments.create" to Target("exponential_comments", "create"),
        "exponential.comments.delete" to Target("exponential_comments", "delete"),
        "exponential.comments.list" to Target("exponential_comments", "list"),
        "exponential.comments.update" to Target("exponential_comments", "update"),
        "exponential.devices.account_login" to Target("exponential_devices", "account_login"),
        "exponential.devices.list" to Target("exponential_devices", "list"),
        "exponential.events.emit" to Target("exponential_events", "emit"),
        "exponential.events.list" to Target("exponential_events", "list"),
        "exponential.issues.activity" to Target("exponential_issues", "activity"),
        "exponential.issues.create" to Target("exponential_issues", "create"),
        "exponential.issues.delete" to Target("exponential_issues", "delete"),
        "exponential.issues.get" to Target("exponential_issues", "get"),
        "exponential.issues.labels_add" to Target("exponential_issues", "labels_add"),
        "exponential.issues.labels_remove" to Target("exponential_issues", "labels_remove"),
        "exponential.issues.list" to Target("exponential_issues", "list"),
        "exponential.issues.relations_add" to Target("exponential_issues", "relations_add"),
        "exponential.issues.relations_remove" to Target("exponential_issues", "relations_remove"),
        "exponential.issues.show" to Target("exponential_issues", "show"),
        "exponential.issues.subscribe" to Target("exponential_issues", "subscribe"),
        "exponential.issues.unsubscribe" to Target("exponential_issues", "unsubscribe"),
        "exponential.issues.update" to Target("exponential_issues", "update"),
        "exponential.issues.update_status" to Target("exponential_issues", "update_status"),
        "exponential.notifications.list" to Target("exponential_notifications", "list"),
        "exponential.notifications.mark_read" to Target("exponential_notifications", "mark_read"),
        "exponential.people.add" to Target("exponential_people", "add"),
        "exponential.people.invites_create" to Target("exponential_people", "invites_create"),
        "exponential.people.invites_list" to Target("exponential_people", "invites_list"),
        "exponential.people.invites_revoke" to Target("exponential_people", "invites_revoke"),
        "exponential.people.list" to Target("exponential_people", "list"),
        "exponential.pr.close" to Target("exponential_pr", "close"),
        "exponential.pr.files" to Target("exponential_pr", "files"),
        "exponential.pr.merge" to Target("exponential_pr", "merge"),
        "exponential.pr.open" to Target("exponential_pr", "open"),
        "exponential.pr.retarget" to Target("exponential_pr", "retarget"),
        "exponential.pr.unlink" to Target("exponential_pr", "unlink"),
        "exponential.pr.update" to Target("exponential_pr", "update"),
        "exponential.sessions.end" to Target("exponential_sessions", "end"),
        "exponential.sessions.get" to Target("exponential_sessions", "get"),
        "exponential.sessions.link" to Target("exponential_sessions", "link"),
        "exponential.sessions.list" to Target("exponential_sessions", "list"),
        "exponential.sessions.send" to Target("exponential_sessions_send", null),
        "exponential.sessions.unlink" to Target("exponential_sessions", "unlink"),
        "exponential.team_config.agents_list" to Target("exponential_team_config", "agents_list"),
        "exponential.team_config.labels_create" to Target("exponential_team_config", "labels_create"),
        "exponential.team_config.labels_delete" to Target("exponential_team_config", "labels_delete"),
        "exponential.team_config.labels_get" to Target("exponential_team_config", "labels_get"),
        "exponential.team_config.labels_list" to Target("exponential_team_config", "labels_list"),
        "exponential.team_config.labels_update" to Target("exponential_team_config", "labels_update"),
        "exponential.team_config.mcp_servers_add" to Target("exponential_team_config", "mcp_servers_add"),
        "exponential.team_config.mcp_servers_list" to Target("exponential_team_config", "mcp_servers_list"),
        "exponential.team_config.mcp_servers_remove" to Target("exponential_team_config", "mcp_servers_remove"),
        "exponential.team_config.prompts_create" to Target("exponential_team_config", "prompts_create"),
        "exponential.team_config.prompts_delete" to Target("exponential_team_config", "prompts_delete"),
        "exponential.team_config.prompts_get" to Target("exponential_team_config", "prompts_get"),
        "exponential.team_config.prompts_list" to Target("exponential_team_config", "prompts_list"),
        "exponential.team_config.prompts_update" to Target("exponential_team_config", "prompts_update"),
        "exponential.team_config.repositories_add" to Target("exponential_team_config", "repositories_add"),
        "exponential.team_config.repositories_list" to Target("exponential_team_config", "repositories_list"),
        "exponential.team_config.statuses_create" to Target("exponential_team_config", "statuses_create"),
        "exponential.team_config.statuses_delete" to Target("exponential_team_config", "statuses_delete"),
        "exponential.team_config.statuses_list" to Target("exponential_team_config", "statuses_list"),
        "exponential.team_config.statuses_update" to Target("exponential_team_config", "statuses_update"),
        "exponential.teams.create" to Target("exponential_teams", "create"),
        "exponential.teams.get" to Target("exponential_teams", "get"),
        "exponential.teams.list" to Target("exponential_teams", "list"),
        "exponential.teams.update" to Target("exponential_teams", "update"),
    )

    /** Every function name, sorted. */
    val ALL: List<String> = TOOLS.keys.toList()

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

    /** May [name] be granted at all: an offered, non-destructive action. */
    fun actionGrantable(name: String): Boolean = parseActionName(name) != null && !actionDestructive(name)

    /** `exponential.issues.update_status` → the tool and op it runs; null for
     *  anything that is not an offered action. */
    fun parseActionName(name: String): Target? = TOOLS[name]

    /** VAPP-107's pattern rule: an exact name or a `prefix*`. */
    private fun matches(pattern: String, name: String): Boolean =
        if (pattern.endsWith("*")) name.startsWith(pattern.dropLast(1)) else pattern == name

    /** The actions a package's `functions` list names (patterns expanded), in
     *  the fixture's order; anything that is not an offered action is dropped. */
    fun declaredActions(functions: List<String>): List<String> =
        ALL.filter { name -> functions.any { matches(it, name) } }

    /** A typed action's MCP `arguments`: `{…args, op}` for a noun's op (the
     *  NAME's op wins: an app-supplied `op` key cannot change the operation the
     *  card and the grant name), the args alone for a verb tool. */
    fun arguments(target: Target, args: Map<String, JsonValue>): Map<String, JsonValue> =
        if (target.op != null) args + mapOf("op" to JsonValue.Str(target.op)) else args

    /** Does the app's `granted_tools` list let [name] run without asking?
     *  Exact names only (a grant is a person's choice, never a pattern), and
     *  a destructive action ignores a stored grant: it always asks. */
    fun grantAllows(granted: List<String>, name: String, args: Map<String, JsonValue>): Boolean =
        actionGrantable(name) && !callDestructive(name, args) && name in granted

    /** An `apps.granted_tools` column (a JSON string[]) as a list; null,
     *  malformed or non-string entries read as none. */
    fun grantedList(json: String?): List<String> =
        json?.let { runCatching { JsonValue.parse(it) }.getOrNull() }?.array?.mapNotNull { it.string } ?: emptyList()

    /** The fixture's `destructiveWords` (`AppActionsTest` locks the copy). */
    internal val DESTRUCTIVE_WORDS = listOf("delete", "merge", "close", "end", "remove", "trash")

    /** The fixture's `destructiveArgs`: an arg that makes a grantable
     *  action's call destructive (`sessions.send` + `stop` = stop-run). */
    internal val DESTRUCTIVE_ARGS: Map<String, List<String>> = mapOf("exponential.sessions.send" to listOf("stop"))

    /** A destructive CALL: a destructive action, or a grantable one whose
     *  args carry a [DESTRUCTIVE_ARGS] key (present and not false/null). */
    fun callDestructive(name: String, args: Map<String, JsonValue>): Boolean =
        actionDestructive(name) || DESTRUCTIVE_ARGS[name].orEmpty().any { key ->
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
