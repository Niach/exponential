package com.exponential.app.domain

/**
 * #VAPP-185: Settings › MCP, ONE page ×4 (`fixtures/settings-mcp.json`, its
 * `$comment` is the spec): Endpoint · Servers · Clients (Mine | Everyone) ·
 * Access (+ Activity) · Built-in Exponential tools. The Android mirror of
 * TS `settings-mcp.ts` (web), desktop `settings/mcp.rs`, iOS
 * `McpSettings.swift`; locked to the fixture by `McpSettingsTest`.
 */
object McpSettings {
    const val TITLE = "MCP"
    val SECTIONS: List<String> = listOf("endpoint", "servers", "clients", "access", "builtin")
    val LABELS: Map<String, String> = linkedMapOf(
        "endpoint" to "Endpoint",
        "servers" to "Servers",
        "clients" to "Clients",
        "access" to "Access",
        "builtin" to "Built-in Exponential tools",
    )
    const val COPY = "Copy"
    const val COPIED = "Copied"
    const val ENDPOINT_PATH = "/api/mcp/apps"
    const val REVOKE = "Revoke"
    const val ACTIVITY = "Activity"

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
        const val ENDPOINT = "settings-mcp-endpoint"
        const val SERVERS = "settings-mcp-servers"
        const val CLIENTS = "settings-mcp-clients"
        const val ACCESS = "settings-mcp-access"
        const val BUILTIN = "settings-mcp-builtin"
        const val ACTIVITY = "settings-mcp-activity"
    }

    /** The endpoint row's URL: the instance origin + [ENDPOINT_PATH]. */
    fun endpointUrl(instanceUrl: String): String = instanceUrl.trimEnd('/') + ENDPOINT_PATH

    /** A server row's "Used by …" caption (null = no app uses it), by name, case-insensitive. */
    fun usedByCaption(apps: List<String>): String? {
        if (apps.isEmpty()) return null
        return USED_BY.replace("{apps}", apps.sortedWith(String.CASE_INSENSITIVE_ORDER).joinToString(", "))
    }

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

    fun builtinTools(): List<BuiltinTool> = DomainContract.expToolNames.indices.map { i ->
        BuiltinTool(
            name = DomainContract.expToolPrefix + DomainContract.expToolNames[i],
            title = DomainContract.expToolTitles[i],
            blurb = DomainContract.expToolBlurbs[i],
        )
    }

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
