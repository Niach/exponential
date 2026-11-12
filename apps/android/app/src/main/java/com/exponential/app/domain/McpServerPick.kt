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

/** The `MCP servers` row's value: the number picked, none at zero. */
fun mcpPickValue(count: Int): String? = if (count > 0) count.toString() else null

// EXP-1793: the picker row's caption + glyph, web `mcpServerPickerItems` /
// `getMcpServerIcon` (packages/ui/src/picker/mcp-server-picker.tsx).

/** A URL's `host` the way web's `new URL(url).host` reads it: lowercased,
 *  the port kept unless it is the scheme's default; null when unparseable. */
fun mcpHostOf(url: String?): String? {
    if (url.isNullOrBlank()) return null
    val uri = runCatching { java.net.URI(url.trim()) }.getOrNull() ?: return null
    val host = uri.host?.lowercase() ?: return null
    val defaultPort = when (uri.scheme?.lowercase()) {
        "http", "ws" -> 80
        "https", "wss" -> 443
        else -> -1
    }
    return if (uri.port == -1 || uri.port == defaultPort) host else "$host:${uri.port}"
}

private fun mcpPathOf(url: String): String? =
    runCatching { java.net.URI(url.trim()).path }.getOrNull()?.ifEmpty { "/" }

/** The contract `mcpCatalog` mark a server's URL belongs to: a hosted entry
 *  by exact host, else a `{host}` template whose path the URL carries. */
fun mcpCatalogMarkFor(url: String?): String? {
    val host = mcpHostOf(url) ?: return null
    val urls = DomainContract.mcpCatalogUrls
    val marks = DomainContract.mcpCatalogMarks
    val isTemplate = { i: Int -> urls[i].contains("{host}") }
    urls.indices.firstOrNull { !isTemplate(it) && mcpHostOf(urls[it]) == host }
        ?.let { return marks[it] }
    val path = mcpPathOf(url!!) ?: return null
    return urls.indices.firstOrNull {
        isTemplate(it) && mcpPathOf(urls[it].replace("{host}", "template.invalid")) == path
    }?.let { marks[it] }
}

/** What a server row leads with. */
sealed interface McpServerGlyph {
    /** A catalog server's real brand mark (contract `mcpCatalog` slug). */
    data class Brand(val mark: String) : McpServerGlyph
    /** A stdio command: `session-shell`. */
    data object Shell : McpServerGlyph
    /** Anything else: `ui-mcp`. */
    data object Plug : McpServerGlyph
}

fun mcpServerGlyph(server: McpServerOption): McpServerGlyph {
    mcpCatalogMarkFor(server.url)?.let { return McpServerGlyph.Brand(it) }
    return if (!server.command.isNullOrEmpty() && server.url.isNullOrEmpty()) {
        McpServerGlyph.Shell
    } else {
        McpServerGlyph.Plug
    }
}

/** The muted second line: the not-ready note, else the host (or the command). */
fun mcpServerCaption(server: McpServerOption): String? =
    mcpNotReadyLabel(server) ?: mcpServerTarget(server)

/** The host (or the command) a row names; also a search keyword. */
fun mcpServerTarget(server: McpServerOption): String? =
    mcpHostOf(server.url) ?: server.command?.takeIf { it.isNotEmpty() }
