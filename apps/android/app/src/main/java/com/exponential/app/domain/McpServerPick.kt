package com.exponential.app.domain

// EXP-792/EXP-1249: the composer's MCP servers pick on a phone (the "+" menu's
// `MCP servers ›` row), the pure half of web `lib/mcp-servers.ts`: the server
// holds each member's credential, so readiness is the CALLER's own
// `connection` — the same on every machine. Same names, same test names.

/** One team server as the picker reads it (`mcpServers.list` row subset). */
data class McpServerOption(
    val id: String,
    val name: String,
    /** The caller's `connection.status`: connected | not_needed | expired | error | missing. */
    val connectionStatus: String,
    val enabledByDefault: Boolean = false,
    val url: String? = null,
    val command: String? = null,
)

/** A run can use the server as the caller stands: nothing to sign in to, or
 *  the caller's own credential is usable. */
fun mcpServerReady(server: McpServerOption): Boolean =
    server.connectionStatus == "connected" || server.connectionStatus == "not_needed"

/** The picker's muted second line for a server the caller cannot use yet. */
fun mcpNotReadyLabel(server: McpServerOption): String? = when (server.connectionStatus) {
    "connected", "not_needed" -> null
    "expired", "error" -> "Reconnect first"
    else -> "Connect first"
}

/** The pick a new composer starts with: the [saved] pick clamped to ready
 *  rows, else the ready rows the team enabled by default. */
fun preselectMcpServerIds(servers: List<McpServerOption>, saved: List<String>?): List<String> {
    val ready = servers.filter(::mcpServerReady)
    if (saved != null) {
        val known = ready.mapTo(HashSet()) { it.id }
        return saved.filter { it in known }
    }
    return ready.filter { it.enabledByDefault }.map { it.id }
}

/** A REAL action owns its MCP list (its own pick rides the run); a builtin
 *  (`builtin:*`), an issue or a chat does not. */
fun subjectOwnsMcpServers(actionId: String?): Boolean =
    actionId != null && !actionId.startsWith("builtin:")

/** The `MCP servers` row's value: the number picked, none at zero. */
fun mcpPickValue(count: Int): String? = if (count > 0) count.toString() else null
