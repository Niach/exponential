package com.exponential.app.domain

import java.io.ByteArrayOutputStream

/**
 * EXP-1188: what a tapped markdown link in agent prose (narration, Guide,
 * thread) opens. Locked ×4 against the contract fixture `app-link.json` (web
 * `lib/app-link.ts`, desktop `domain::app_link`, iOS ExpCore `AppLink.swift`).
 * Hand-parsed — no `android.net.Uri` — so the rule runs in JVM unit tests.
 */
sealed interface AppLink {
    /** An issue on this instance: `/t/{team}/boards/{board}/issues/{identifier}`. */
    data class Issue(val teamSlug: String, val boardSlug: String, val identifier: String) : AppLink

    /** #EXP-1674: an issue by identifier alone, `/t/{team}/issues/{identifier}`. */
    data class IssueRef(val teamSlug: String, val identifier: String) : AppLink

    /** #EXP-1674: `/t/{team}/boards/{board}`. */
    data class Board(val teamSlug: String, val boardSlug: String) : AppLink

    /** A run on this instance: `/t/{team}/sessions/{id}`. */
    data class Session(val teamSlug: String, val sessionId: String) : AppLink

    /** #EXP-1674: THE plan page, `/t/{team}/sessions/{id}/plan/{messageId}`. */
    data class Plan(val teamSlug: String, val sessionId: String, val messageId: String) : AppLink

    /** EXP-1284: a thread on this instance. */
    data class Thread(val teamSlug: String, val threadId: String) : AppLink

    /** #EXP-1674: the chats list, `/t/{team}/threads`. */
    data class Threads(val teamSlug: String) : AppLink

    /** #EXP-1674: an agent's chat, `/t/{team}/agents/{id}`. */
    data class Agent(val teamSlug: String, val agentId: String) : AppLink

    /** #EXP-1674: the team's agent page, `/t/{team}/agent`. */
    data class AgentHome(val teamSlug: String) : AppLink

    /** #EXP-1674: `/t/{team}/inbox`. */
    data class Inbox(val teamSlug: String) : AppLink

    /** #EXP-1674: `/t/{team}/devices` or `/t/{team}/settings/devices`. */
    data class Devices(val teamSlug: String) : AppLink

    /** #EXP-1674: `/t/{team}/settings[/{section}]`; [section] = "" for the root. */
    data class Settings(val teamSlug: String, val section: String) : AppLink

    /** #EXP-1674: an app's first screen in THE RUN VIEW, `/t/{team}/apps/{app}`. */
    data class AppHome(val teamSlug: String, val appSlug: String) : AppLink

    /** VAPP-218: an app screen, `/t/{team}/apps/{app}/{screen}?<params>`:
     *  THE RUN VIEW showing that screen. */
    data class AppScreen(val teamSlug: String, val appSlug: String, val screen: String, val params: Map<String, String>) : AppLink

    /** Any other page on this instance, the path as written (query + hash kept). */
    data class App(val path: String) : AppLink

    /** Another site — opens in the browser exactly as written. */
    data class External(val url: String) : AppLink

    /** Not a link worth opening (placeholder hosts, other schemes, `#x`, …). */
    data object Ignore : AppLink
}

/** #EXP-1674: `app-link.json` `copy.noAccess`. */
const val LINK_NO_ACCESS = "Not found or no access"

/** #EXP-1674: what opening a resolved in-app link does (`openCases`). */
enum class LinkOpenOutcome(val wire: String) {
    Open("open"),
    SwitchTeam("switchTeam"),
    NoAccess("noAccess"),
    Browser("browser"),
    NotFound("notFound"),
}

/** #EXP-1674: what a chat card's link may open (plan-card.json `linkCases`): in-app, or https out. */
fun cardLinkOpens(href: String, origin: String, team: String?): Boolean =
    when (val link = classifyAppLink(href, origin, team)) {
        AppLink.Ignore -> false
        is AppLink.External -> link.url.startsWith("https://", ignoreCase = true)
        else -> true
    }

/** #EXP-1674: the ONE open rule ×4; unreadable looks exactly like missing. */
fun linkOpenOutcome(member: Boolean, activeTeam: Boolean, found: Boolean, webOnly: Boolean): LinkOpenOutcome = when {
    !member -> LinkOpenOutcome.NoAccess
    webOnly -> LinkOpenOutcome.Browser
    !found -> LinkOpenOutcome.NotFound
    activeTeam -> LinkOpenOutcome.Open
    else -> LinkOpenOutcome.SwitchTeam
}

/** #EXP-1674: `~/x` = `/t/<team>/x`, or null when it cannot be expanded (no
 *  team, not `~/`, `~//`, a `.` or `..` segment even percent-encoded). */
fun expandTeamLink(href: String, team: String?): String? {
    if (team.isNullOrEmpty() || !href.startsWith("~/") || href.startsWith("~//")) return null
    val rest = href.substring(2)
    val segments = beforeQueryAndHash(rest).split('/').map(::strictDecode)
    if (segments.any { it == "." || it == ".." }) return null
    return "/t/${encodeUriComponent(team)}/$rest"
}

/** The team a link names: every team route's, or an `app` path's under
 *  `/t/<team>/`; null off the team tree. */
fun appLinkTeam(link: AppLink): String? = when (link) {
    is AppLink.Issue -> link.teamSlug
    is AppLink.IssueRef -> link.teamSlug
    is AppLink.Board -> link.teamSlug
    is AppLink.Session -> link.teamSlug
    is AppLink.Plan -> link.teamSlug
    is AppLink.Thread -> link.teamSlug
    is AppLink.Threads -> link.teamSlug
    is AppLink.Agent -> link.teamSlug
    is AppLink.AgentHome -> link.teamSlug
    is AppLink.Inbox -> link.teamSlug
    is AppLink.Devices -> link.teamSlug
    is AppLink.Settings -> link.teamSlug
    is AppLink.AppHome -> link.teamSlug
    is AppLink.AppScreen -> link.teamSlug
    is AppLink.App -> pathSegments(link.path).takeIf { it.size >= 2 && it[0] == "t" }?.get(1)
    is AppLink.External, AppLink.Ignore -> null
}

/** `/t/<team>/apps`: the app path this client has a screen for (the Apps tab). */
fun isAppsListPath(path: String): Boolean {
    val segments = pathSegments(path)
    return segments.size == 3 && segments[0] == "t" && segments[2] == "apps"
}

fun classifyAppLink(href: String, origin: String, team: String? = null): AppLink {
    val trimmed = href.trim()
    if (trimmed.isEmpty()) return AppLink.Ignore
    if (trimmed.startsWith("~")) {
        return expandTeamLink(trimmed, team)?.let(::inAppLink) ?: AppLink.Ignore
    }
    if (trimmed.startsWith("/")) {
        if (trimmed.startsWith("//")) return AppLink.Ignore
        return inAppLink(trimmed)
    }
    if (trimmed.regionMatches(0, "mailto:", 0, 7, ignoreCase = true)) return AppLink.External(trimmed)
    val schemeEnd = when {
        trimmed.regionMatches(0, "https://", 0, 8, ignoreCase = true) -> 8
        trimmed.regionMatches(0, "http://", 0, 7, ignoreCase = true) -> 7
        else -> return AppLink.Ignore
    }
    val rest = trimmed.substring(schemeEnd)
    val authorityEnd = rest.indexOfFirst { it == '/' || it == '?' || it == '#' }
        .let { if (it < 0) rest.length else it }
    val authority = rest.substring(0, authorityEnd)
    if (!isValidAuthority(authority)) return AppLink.Ignore
    val schemeAndAuthority = trimmed.substring(0, schemeEnd + authorityEnd)
    if (!schemeAndAuthority.equals(origin.trim().trimEnd('/'), ignoreCase = true)) {
        return AppLink.External(trimmed)
    }
    val path = rest.substring(authorityEnd)
    return inAppLink(if (path.startsWith("/")) path else "/$path")
}

/** `[A-Za-z0-9-]` labels joined by `.` (two or more, or `localhost`), an
 *  optional `:port`, no userinfo. */
private fun isValidAuthority(authority: String): Boolean {
    val colon = authority.indexOf(':')
    val host = if (colon < 0) authority else authority.substring(0, colon)
    if (colon >= 0) {
        val port = authority.substring(colon + 1)
        if (port.isEmpty() || !port.all { it in '0'..'9' }) return false
    }
    if (host.isEmpty()) return false
    val labels = host.split('.')
    // Two labels at least (or `localhost`): a browser punycodes the
    // placeholder `https://…` into the one-label host `xn--rvg`.
    if (labels.size < 2 && !host.equals("localhost", ignoreCase = true)) return false
    return labels.all { label ->
        label.isNotEmpty() && label.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '-' }
    }
}

private fun beforeQueryAndHash(path: String): String =
    path.substring(0, path.indexOfFirst { it == '?' || it == '#' }.let { if (it < 0) path.length else it })

private fun pathSegments(path: String): List<String> =
    beforeQueryAndHash(path).split('/').filter { it.isNotEmpty() }.map(::strictDecode)

/** An in-app path (query + hash kept for [AppLink.App], dropped for the routes). */
private fun inAppLink(path: String): AppLink {
    val segments = pathSegments(path)
    val n = segments.size
    if (n >= 3 && segments[0] == "t") {
        val teamSlug = segments[1]
        val kind = segments[2]
        val a = segments.getOrNull(3)
        val b = segments.getOrNull(4)
        val c = segments.getOrNull(5)
        if (kind == "issues" && a != null && n == 4) return AppLink.IssueRef(teamSlug, a)
        if (kind == "boards" && a != null && b == "issues" && c != null && n == 6) {
            return AppLink.Issue(teamSlug = teamSlug, boardSlug = a, identifier = c)
        }
        if (kind == "boards" && a != null && n == 4) return AppLink.Board(teamSlug, a)
        if (kind == "sessions" && a != null && b == "plan" && c != null && n == 6) {
            return AppLink.Plan(teamSlug, a, c)
        }
        if (kind == "sessions" && a != null && n == 4) return AppLink.Session(teamSlug, a)
        if (kind == "threads" && a != null && n == 4) return AppLink.Thread(teamSlug, a)
        if (kind == "threads" && n == 3) return AppLink.Threads(teamSlug)
        if (kind == "agents" && a != null && n == 4) return AppLink.Agent(teamSlug, a)
        if (kind == "agent" && n == 3) return AppLink.AgentHome(teamSlug)
        if (kind == "inbox" && n == 3) return AppLink.Inbox(teamSlug)
        if (kind == "devices" && n == 3) return AppLink.Devices(teamSlug)
        if (kind == "settings" && a == "devices" && n == 4) return AppLink.Devices(teamSlug)
        if (kind == "settings" && a != null && n == 4) return AppLink.Settings(teamSlug, a)
        if (kind == "settings" && n == 3) return AppLink.Settings(teamSlug, "")
        // VAPP-218: an app screen, its query = the screen's params.
        if (kind == "apps" && a != null && b != null && n == 5) {
            return AppLink.AppScreen(teamSlug = teamSlug, appSlug = a, screen = b, params = screenQueryParams(path))
        }
        if (kind == "apps" && a != null && n == 4) return AppLink.AppHome(teamSlug, a)
    }
    return AppLink.App(path)
}

/** `encodeURIComponent`: UTF-8, everything but `A-Za-z0-9-_.!~*'()` percent-encoded. */
private fun encodeUriComponent(text: String): String {
    val hex = "0123456789ABCDEF"
    val out = StringBuilder()
    for (byte in text.toByteArray(Charsets.UTF_8)) {
        val value = byte.toInt() and 0xFF
        val c = value.toChar()
        if (c in 'A'..'Z' || c in 'a'..'z' || c in '0'..'9' || c in "-_.!~*'()") {
            out.append(c)
        } else {
            out.append('%').append(hex[value shr 4]).append(hex[value and 0x0F])
        }
    }
    return out.toString()
}

/** VAPP-218: a path's query as screen params: `k=v` pairs split on `&` then
 *  the first `=`, each side percent-decoded (a bad escape kept, no `+`
 *  rewriting), empty keys dropped, the last repeat wins, the editor's own
 *  keys ([Apps.Screens.RESERVED]) dropped; sorted by key. */
internal fun screenQueryParams(path: String): Map<String, String> {
    val q = path.indexOf('?')
    if (q < 0) return emptyMap()
    val h = path.indexOf('#', q)
    val query = if (h < 0) path.substring(q + 1) else path.substring(q + 1, h)
    val found = HashMap<String, String>()
    for (pair in query.split('&')) {
        val eq = pair.indexOf('=')
        val key = strictDecode(if (eq < 0) pair else pair.substring(0, eq))
        if (key.isEmpty() || key in Apps.Screens.RESERVED) continue
        found[key] = if (eq < 0) "" else strictDecode(pair.substring(eq + 1))
    }
    return found.toSortedMap()
}

/** VAPP-218: `decodeURIComponent` or the text as written (a bad escape or
 *  invalid UTF-8 keeps the WHOLE text, like the TS reference). */
private fun strictDecode(text: String): String {
    if ('%' !in text) return text
    val out = ByteArrayOutputStream()
    var i = 0
    while (i < text.length) {
        val c = text[i]
        if (c == '%') {
            if (i + 2 >= text.length) return text
            val hi = Character.digit(text[i + 1], 16)
            val lo = Character.digit(text[i + 2], 16)
            if (hi < 0 || lo < 0) return text
            out.write(hi * 16 + lo)
            i += 3
            continue
        }
        val cp = text.codePointAt(i)
        out.write(String(Character.toChars(cp)).toByteArray(Charsets.UTF_8))
        i += Character.charCount(cp)
    }
    val bytes = out.toByteArray()
    return try {
        java.nio.charset.StandardCharsets.UTF_8.newDecoder()
            .onMalformedInput(java.nio.charset.CodingErrorAction.REPORT)
            .onUnmappableCharacter(java.nio.charset.CodingErrorAction.REPORT)
            .decode(java.nio.ByteBuffer.wrap(bytes)).toString()
    } catch (_: java.nio.charset.CharacterCodingException) {
        text
    }
}
