package com.exponential.app.domain

/**
 * #VAPP-185: Settings › MCP, ONE page ×4 (`fixtures/settings-mcp.json`, its
 * `$comment` is the spec): Overview · Servers (the built-ins first, every row
 * expandable to its tools) · Access (+ Activity) · Clients (Mine | Everyone).
 * The Android mirror of
 * TS `settings-mcp.ts` (web), desktop `settings/mcp.rs`, iOS
 * `McpSettings.swift`; locked to the fixture by `McpSettingsTest`.
 */
object McpSettings {
    const val TITLE = "MCP"
    val SECTIONS: List<String> = listOf("overview", "servers", "access", "clients")
    val LABELS: Map<String, String> = linkedMapOf(
        "servers" to "Servers",
        "clients" to "Clients",
        "access" to "Access",
    )
    const val OVERVIEW = "MCP servers your team's sessions, agents and apps can call."
    const val COPY = "Copy"
    const val COPIED = "Copied"
    const val REVOKE = "Revoke"
    const val ACTIVITY = "Activity"
    /** #FEED-242: the owner's last Access row, the app's default level. */
    const val EVERYONE_ELSE = "Everyone else"

    data class Option(val value: String, val label: String)

    val SCOPES: List<Option> = listOf(Option("mine", "Mine"), Option("everyone", "Everyone"))
    val LEVELS: List<Option> = listOf(Option("none", "None"), Option("read", "Read only"), Option("write", "Full"))
    val KIND_LABELS: Map<String, String> = linkedMapOf("builtin" to "Exponential tools", "apps" to "Apps")
    val ACCESS_LABELS: Map<String, String> = linkedMapOf("read" to "Read only", "full" to "Full")
    val STATUS_LABELS: Map<String, String> = linkedMapOf(
        "ok" to "OK",
        "denied" to "Denied",
        "invalid" to "Invalid",
        "failed" to "Failed",
        "upstream" to "Upstream",
    )
    const val NEVER = "never used"
    const val BUILTIN_APP = "Exponential"
    const val FIRST_PARTY = "API key"
    const val USED_BY = "Used by {apps}"
    const val TOOL_COUNT_ONE = "{n} tool"
    const val TOOL_COUNT_OTHER = "{n} tools"

    object TestIds {
        const val PAGE = "settings-mcp-page"
        const val OVERVIEW = "settings-mcp-overview"
        const val SERVERS = "settings-mcp-servers"
        const val BUILTIN = "settings-mcp-builtin"
        const val APPS_SERVER = "settings-mcp-apps-server"
        const val SERVER_TOOLS = "settings-mcp-server-tools"
        const val ACCESS = "settings-mcp-access"
        const val ACTIVITY = "settings-mcp-activity"
        const val CLIENTS = "settings-mcp-clients"
    }

    // ── Owner 2026-11-12: ONE Servers group, every row expandable to its tools ──

    /** A built-in server row (`builtins.<id>`): its name and its URL path. */
    data class Builtin(val id: String, val name: String, val path: String)

    val BUILTIN_EXPONENTIAL = Builtin("exponential", "Exponential", "/api/mcp")
    val BUILTIN_APPS = Builtin("apps", "Team apps", "/api/mcp/apps")
    val BUILTINS: List<Builtin> = listOf(BUILTIN_EXPONENTIAL, BUILTIN_APPS)

    const val BUILTIN_PILL = "Built-in"
    const val TOOLS_CONNECT = "Connect to see its tools."
    const val TOOLS_STDIO = "A local command: its tools list where a session runs it."
    const val TOOLS_FAILED = "Could not list its tools: {reason}"
    const val TOOLS_NONE = "No tools."
    const val RETRY = "Retry"

    /** A built-in's URL: the instance origin + its path (TS `builtinEndpoint`). */
    fun builtinEndpoint(instanceUrl: String, builtin: Builtin): String = instanceUrl.trimEnd('/') + builtin.path

    /** The Servers group's order (TS `mcpServerOrder`): `builtin:exponential`,
     *  `builtin:apps` when an app exposes tools, then the team's servers. */
    fun <T> serverOrder(appTools: Boolean, servers: List<T>): List<Any?> =
        listOf<Any?>("builtin:exponential") + (if (appTools) listOf("builtin:apps") else emptyList()) + servers

    /** What an open team server row shows (TS `mcpToolsState`). */
    enum class ToolsState(val wire: String) { List("list"), Connect("connect"), Stdio("stdio") }

    fun toolsState(transport: String, status: String): ToolsState = when {
        transport != "http" -> ToolsState.Stdio
        status == "connected" || status == "not_needed" -> ToolsState.List
        else -> ToolsState.Connect
    }

    /** "Could not list its tools: <reason>". */
    fun toolsFailedLine(reason: String): String = TOOLS_FAILED.replace("{reason}", reason)

    /** A server row's "Used by …" caption (null = no app uses it), by name, case-insensitive. */
    fun usedByCaption(apps: List<String>): String? {
        if (apps.isEmpty()) return null
        return USED_BY.replace("{apps}", apps.sortedWith(String.CASE_INSENSITIVE_ORDER).joinToString(", "))
    }

    /** #VAPP-239 (`runsOn`): a device-routed server row's caption part. */
    const val RUNS_ON = "Runs on {device}"

    /** #VAPP-239 (`runsOnCases`): "Runs on <device>", null = the server broker calls it. */
    fun runsOnCaption(device: String?): String? = device?.takeIf { it.isNotEmpty() }?.let { RUNS_ON.replace("{device}", it) }

    /** A client row's caption: [member ·] kinds (apps access) · last used. */
    fun clientCaption(
        scope: String,
        userName: String,
        kinds: List<String>,
        appsAccess: String?,
        /** The compact relative time ("2h"), null = never. */
        lastUsed: String?,
    ): String {
        val kindText = listOf("builtin", "apps").filter { it in kinds }.joinToString(", ") { kind ->
            val label = KIND_LABELS.getValue(kind)
            val access = appsAccess?.let { ACCESS_LABELS[it] }
            if (kind == "apps" && access != null) "$label ($access)" else label
        }
        return listOfNotNull(if (scope == "everyone") userName else null, kindText, lastUsed ?: NEVER)
            .filter { it.isNotEmpty() }
            .joinToString(" · ")
    }

    /** An activity row's caption: app · [member ·] client · when. */
    fun activityCaption(owner: Boolean, appName: String?, userName: String, clientName: String?, `when`: String): String =
        listOfNotNull(appName ?: BUILTIN_APP, if (owner) userName else null, clientName ?: FIRST_PARTY, `when`)
            .filter { it.isNotEmpty() }
            .joinToString(" · ")

    /** "1 tool" | "3 tools". */
    fun toolCountLabel(n: Int): String = (if (n == 1) TOOL_COUNT_ONE else TOOL_COUNT_OTHER).replace("{n}", n.toString())

    /** The label of a policy level. */
    fun levelLabel(level: String): String = LEVELS.firstOrNull { it.value == level }?.label ?: level

    /** An activity row's Pill word. */
    fun statusLabel(status: String): String = STATUS_LABELS[status] ?: status

    /** One built-in Exponential tool row (web `builtinExpTools`): contract order. */
    data class BuiltinTool(val name: String, val title: String, val blurb: String)

    /** #EXP-1657: one band of the open Exponential row = one contract tool group. */
    data class BuiltinToolBand(val id: String, val label: String, val tools: List<BuiltinTool>)

    /** The listed rows: every contract row but the retired `aliasOf` ones
     *  (their `expToolRowGroups` entry is ""), in contract order. */
    fun builtinTools(): List<BuiltinTool> = DomainContract.expToolNames.indices
        .filter { DomainContract.expToolRowGroups[it].isNotEmpty() }
        .map(::builtinTool)

    private fun builtinTool(i: Int) = BuiltinTool(
        name = DomainContract.expToolPrefix + DomainContract.expToolNames[i],
        title = DomainContract.expToolTitles[i],
        blurb = DomainContract.expToolBlurbs[i],
    )

    /** The bands the open Exponential row shows for [query] (TS `builtinToolBands`):
     *  contract group order and labels, rows filtered by [filterBuiltinTools],
     *  a band with no match left out (all gone = the empty state). */
    fun builtinToolBands(query: String): List<BuiltinToolBand> =
        DomainContract.expToolGroupIds.mapIndexed { g, id ->
            val rows = DomainContract.expToolNames.indices
                .filter { DomainContract.expToolRowGroups[it] == id }
                .map(::builtinTool)
            BuiltinToolBand(id, DomainContract.expToolGroupLabels[g], filterBuiltinTools(rows, query))
        }.filter { it.tools.isNotEmpty() }

    const val BUILTIN_FILTER_PLACEHOLDER = "Filter tools"

    /** The band's filter: case-insensitive substring over title, blurb and wire name. */
    fun filterBuiltinTools(tools: List<BuiltinTool>, query: String): List<BuiltinTool> {
        val needle = query.trim().lowercase()
        if (needle.isEmpty()) return tools
        return tools.filter {
            it.title.lowercase().contains(needle) || it.blurb.lowercase().contains(needle) || it.name.lowercase().contains(needle)
        }
    }
}
