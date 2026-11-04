package com.exponential.app.domain

/**
 * #EXP-1674: a bare in-app URL in agent prose reads as the chip of what it
 * names. Locked ×4 against the contract fixture `prose-links.json`, whose
 * `description` is the rule (web `lib/prose-links.ts`, desktop
 * `domain::prose_links`, iOS ExpCore `ProseLinks.swift`). Display text only:
 * nothing here ever serializes.
 */
object ProseLinks {
    enum class ChipKind(val wire: String, val icon: String) {
        Issue("issue", "ui-issue"),
        Board("board", "nav-boards"),
        Session("session", "coding-running"),
        Agent("agent", "settings-agents"),
    }

    private const val MARKER = "#exp-chip-"
    private const val STOPS = " \t<>()[]`\"'"
    private const val TRAILING = ".,;:!?*_~"
    private const val ESCAPED = "\\[]*_`~"

    private fun isWord(ch: Char): Boolean =
        ch in '0'..'9' || ch in 'A'..'Z' || ch in 'a'..'z' || ch == '_'

    /** The first three characters of a fence line (at most 3 leading spaces). */
    private fun fenceMark(line: String): String? {
        var spaces = 0
        while (spaces < 3 && spaces < line.length && line[spaces] == ' ') spaces += 1
        val rest = line.substring(spaces)
        return if (rest.startsWith("```") || rest.startsWith("~~~")) rest.substring(0, 3) else null
    }

    private fun chip(url: String, origin: String, team: String, resolve: (ChipKind, String) -> String?): String? {
        val (slug, kind, id) = when (val link = classifyAppLink(url, origin, team)) {
            is AppLink.Issue -> Triple(link.teamSlug, ChipKind.Issue, link.identifier)
            is AppLink.IssueRef -> Triple(link.teamSlug, ChipKind.Issue, link.identifier)
            is AppLink.Board -> Triple(link.teamSlug, ChipKind.Board, link.boardSlug)
            is AppLink.Session -> Triple(link.teamSlug, ChipKind.Session, link.sessionId)
            is AppLink.Thread -> Triple(link.teamSlug, ChipKind.Session, link.threadId)
            is AppLink.Agent -> Triple(link.teamSlug, ChipKind.Agent, link.agentId)
            else -> return null
        }
        if (!slug.equals(team, ignoreCase = true)) return null
        val title = resolve(kind, id) ?: return null
        if (kind == ChipKind.Issue) return "#$id"
        val label = buildString {
            for (ch in title) {
                if (ch in ESCAPED) append('\\')
                append(ch)
            }
        }
        return "[$label](${url.substringBefore('#')}$MARKER${kind.wire})"
    }

    private fun chipLine(line: String, origin: String, team: String, resolve: (ChipKind, String) -> String?): String {
        val out = StringBuilder(line.length)
        var at = 0
        var copied = 0
        while (at < line.length) {
            if (line[at] == '`') {
                var run = at
                while (run < line.length && line[run] == '`') run += 1
                val width = run - at
                var close = run
                var end = -1
                while (close < line.length) {
                    if (line[close] != '`') {
                        close += 1
                        continue
                    }
                    var closeRun = close
                    while (closeRun < line.length && line[closeRun] == '`') closeRun += 1
                    if (closeRun - close == width) {
                        end = closeRun
                        break
                    }
                    close = closeRun
                }
                at = if (end == -1) run else end
                continue
            }
            if (line.startsWith("http://", at, ignoreCase = true) || line.startsWith("https://", at, ignoreCase = true)) {
                val prev = line.getOrNull(at - 1)
                val destination = prev == '(' && line.getOrNull(at - 2) == ']'
                val glued = prev != null && (isWord(prev) || prev == '[' || prev == '<')
                if (!glued && !destination) {
                    var end = at
                    while (end < line.length && line[end] !in STOPS) end += 1
                    while (end > at && line[end - 1] in TRAILING) end -= 1
                    val chip = chip(line.substring(at, end), origin, team, resolve)
                    if (chip != null) {
                        out.append(line, copied, at).append(chip)
                        copied = end
                    }
                    at = end
                    continue
                }
            }
            at += 1
        }
        out.append(line, copied, line.length)
        return out.toString()
    }

    /**
     * [text] with its bare in-app URLs rewritten to their chips' sources.
     * [resolve] = the display title of a SYNCED row, null otherwise.
     */
    fun chipBareAppLinks(text: String, origin: String, team: String?, resolve: (ChipKind, String) -> String?): String {
        if (team.isNullOrEmpty()) return text
        if (!text.contains("http://", ignoreCase = true) && !text.contains("https://", ignoreCase = true)) return text
        var fence: String? = null
        return text.split("\n").joinToString("\n") { line ->
            val mark = fenceMark(line)
            when {
                mark != null -> {
                    if (fence == null) fence = mark else if (fence == mark) fence = null
                    line
                }
                fence == null -> chipLine(line, origin, team, resolve)
                else -> line
            }
        }
    }

    /** The chip kind a rewritten link's href carries (null: an ordinary link). */
    fun chipKind(href: String): ChipKind? {
        val at = href.lastIndexOf(MARKER)
        if (at == -1) return null
        return when (href.substring(at + MARKER.length)) {
            "board" -> ChipKind.Board
            "session" -> ChipKind.Session
            "agent" -> ChipKind.Agent
            else -> null
        }
    }
}
