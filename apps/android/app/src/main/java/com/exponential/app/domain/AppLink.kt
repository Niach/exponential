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

    /** A run on this instance: `/t/{team}/sessions/{id}`. */
    data class Session(val teamSlug: String, val sessionId: String) : AppLink

    /** Any other page on this instance, the path as written (query + hash kept). */
    data class App(val path: String) : AppLink

    /** Another site — opens in the browser exactly as written. */
    data class External(val url: String) : AppLink

    /** Not a link worth opening (placeholder hosts, other schemes, `#x`, …). */
    data object Ignore : AppLink
}

fun classifyAppLink(href: String, origin: String): AppLink {
    val trimmed = href.trim()
    if (trimmed.isEmpty()) return AppLink.Ignore
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

private fun inAppLink(path: String): AppLink {
    val pathOnly = path.substring(0, path.indexOfFirst { it == '?' || it == '#' }.let { if (it < 0) path.length else it })
    val segments = pathOnly.split('/').filter { it.isNotEmpty() }.map(::percentDecode)
    if (segments.size == 6 && segments[0] == "t" && segments[2] == "boards" && segments[4] == "issues") {
        return AppLink.Issue(teamSlug = segments[1], boardSlug = segments[3], identifier = segments[5])
    }
    if (segments.size == 4 && segments[0] == "t" && segments[2] == "sessions") {
        return AppLink.Session(teamSlug = segments[1], sessionId = segments[3])
    }
    return AppLink.App(path)
}

/** Percent-decodes UTF-8 bytes; a malformed escape stays literal. */
private fun percentDecode(segment: String): String {
    if ('%' !in segment) return segment
    val out = ByteArrayOutputStream()
    var i = 0
    while (i < segment.length) {
        val c = segment[i]
        if (c == '%' && i + 2 < segment.length) {
            val hi = Character.digit(segment[i + 1], 16)
            val lo = Character.digit(segment[i + 2], 16)
            if (hi >= 0 && lo >= 0) {
                out.write(hi * 16 + lo)
                i += 3
                continue
            }
        }
        out.write(c.toString().toByteArray(Charsets.UTF_8))
        i++
    }
    return out.toString(Charsets.UTF_8.name())
}
