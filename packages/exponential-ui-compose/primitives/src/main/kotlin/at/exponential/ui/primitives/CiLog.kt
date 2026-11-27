package at.exponential.ui.primitives

/**
 * #EXP-2204 (packages/exponential-ui/docs/ci-components-contract.md §1-§3,
 * §6): the pure rules of LogView/LogLines and the CodeBlock anchors + blame,
 * the SDK's src/ci-log.ts (eui-core `ci_log.rs`, Swift `CiLog.swift`);
 * `CiLogTest` replays fixtures/ci-log.json through it. Inputs are plain
 * Kotlin JSON values (`Map<String, Any?>`, `List<Any?>`, `Number`, `String`,
 * `Boolean`, null); offsets are Unicode code points, like the reference.
 */
object CiLog {
    // §1 The status table

    /** The `ciStatus` enum, in catalogue order. */
    val STATUSES = listOf("passed", "failed", "running", "queued", "cancelled", "skipped")

    /** style.json `ciStatus.spinMs`: one turn of the running glyph. */
    const val SPIN_MS = 1000.0

    /** style.json `logView` / `codeBlock` (px, lines). */
    const val SEARCH_WIDTH = 240.0
    const val INDENT = 16.0
    const val MAX_LINES = 24.0
    const val BLAME_WIDTH = 200.0

    /** One row of THE status table: glyph, spin, mark + text colour tokens (the neutral theme's), the a11y string id. */
    data class StatusEntry(val status: String, val icon: String, val spin: Boolean, val color: String, val text: String, val label: String)

    private val table = mapOf(
        "passed" to StatusEntry("passed", "ci-passed", false, "\$color.success", "\$color.foreground", "statusPassed"),
        "failed" to StatusEntry("failed", "ci-failed", false, "\$color.destructive", "\$color.foreground", "statusFailed"),
        "running" to StatusEntry("running", "ci-running", true, "\$color.info", "\$color.foreground", "statusRunning"),
        "queued" to StatusEntry("queued", "ci-queued", false, "\$color.mutedForeground", "\$color.foreground", "statusQueued"),
        "cancelled" to StatusEntry("cancelled", "ci-skipped", false, "\$color.mutedForeground", "\$color.mutedForeground", "statusCancelled"),
        "skipped" to StatusEntry("skipped", "ci-skipped", false, "\$color.mutedForeground", "\$color.mutedForeground", "statusSkipped"),
    )

    /** A status value → a `ciStatus`: unknown or missing = `queued`. */
    fun status(value: Any?): String = (value as? String)?.takeIf { it in STATUSES } ?: "queued"

    /** THE table row of a status (unknown or missing = `queued`). */
    fun entry(value: Any?): StatusEntry = table.getValue(status(value))

    // §2 Line anchors

    /** A 1-based inclusive line range. */
    data class LineRange(val start: Int, val end: Int) {
        operator fun contains(line: Int): Boolean = line in start..end
    }

    private fun anchorNumber(t: String, from: Int): Pair<Int, Int>? {
        var i = from
        while (i < t.length && t[i] in '0'..'9') i++
        if (i == from || i - from > 9) return null
        val v = t.substring(from, i).toInt()
        return if (v >= 1) v to i else null
    }

    /** `L12` / `L6-L9` (an optional leading `#`) → the normalised range. */
    fun parseAnchor(value: Any?): LineRange? {
        val t = value as? String ?: return null
        var i = if (t.startsWith('#')) 1 else 0
        if (i >= t.length || t[i] != 'L') return null
        val a = anchorNumber(t, i + 1) ?: return null
        i = a.second
        if (i == t.length) return LineRange(a.first, a.first)
        if (i + 1 >= t.length || t[i] != '-' || t[i + 1] != 'L') return null
        val c = anchorNumber(t, i + 2) ?: return null
        if (c.second != t.length) return null
        return LineRange(minOf(a.first, c.first), maxOf(a.first, c.first))
    }

    /** `L<start>` when equal, else `L<min>-L<max>`. */
    fun formatAnchor(start: Int, end: Int? = null): String {
        val e = end ?: start
        val lo = minOf(start, e)
        val hi = maxOf(start, e)
        return if (lo == hi) "L$lo" else "L$lo-L$hi"
    }

    /** A press on [line]: no shift or no valid current anchor → `L<line>`; shift → the range from the current START to [line]. */
    fun extendAnchor(current: Any?, line: Int, shift: Boolean): String {
        if (shift) parseAnchor(current)?.let { return formatAnchor(it.start, line) }
        return formatAnchor(line)
    }

    // §3 The log model

    /** One content line (marker lines get none). [section] -1 = outside every section; [time] `HH:MM:SS` as written; [kind] error|warning|comment; [text] without timestamp + prefix, ANSI kept. */
    data class Line(val n: Int, val section: Int, val time: String?, val kind: String?, val text: String)

    data class Section(val index: Int, val title: String, var start: Int, var end: Int, var count: Int, var status: String, var duration: String?)

    data class Model(val lines: List<Line> = emptyList(), val sections: List<Section> = emptyList()) {
        /** The content line [n] (1-based). */
        fun line(n: Int): Line? = lines.getOrNull(n - 1)?.takeIf { it.n == n }
    }

    /** The end index (exclusive) of a COMPLETE CSI escape at [i] in code points [t], else -1. */
    private fun csiEnd(t: IntArray, i: Int): Int {
        if (i + 1 >= t.size || t[i] != 0x1B || t[i + 1] != '['.code) return -1
        var j = i + 2
        while (j < t.size && t[j] in 0x30..0x3F) j++
        while (j < t.size && t[j] in 0x20..0x2F) j++
        return if (j < t.size && t[j] in 0x40..0x7E) j + 1 else -1
    }

    private fun IntArray.text(from: Int = 0, to: Int = size): String = String(this, from, to - from)

    /** The text with every complete ANSI CSI escape removed. */
    fun stripAnsi(text: String): String {
        if (text.indexOf('\u001b') < 0) return text
        val t = text.codePoints().toArray()
        val out = StringBuilder(text.length)
        var i = 0
        while (i < t.size) {
            val end = csiEnd(t, i)
            if (end >= 0) { i = end; continue }
            out.appendCodePoint(t[i])
            i++
        }
        return out.toString()
    }

    /** [text] with its first [count] escape-free code points removed, every escape kept where it was. */
    private fun cutVisible(text: String, count: Int): String {
        val t = text.codePoints().toArray()
        val out = StringBuilder(text.length)
        var removed = 0
        var i = 0
        while (i < t.size) {
            val end = csiEnd(t, i)
            if (end >= 0) {
                out.append(t.text(i, end))
                i = end
                continue
            }
            if (removed < count) removed++ else out.appendCodePoint(t[i])
            i++
        }
        return out.toString()
    }

    /** `YYYY-MM-DDTHH:MM:SS(.fraction)?(Z|±HH:MM) ` at the start: `HH:MM:SS` and the length including the space (ASCII, so chars). */
    private fun timestamp(b: String): Pair<String, Int>? {
        fun digits(from: Int, n: Int): Boolean = b.length >= from + n && (from until from + n).all { b[it] in '0'..'9' }
        fun at(i: Int, c: Char): Boolean = i < b.length && b[i] == c
        if (!(digits(0, 4) && at(4, '-') && digits(5, 2) && at(7, '-') && digits(8, 2) && at(10, 'T') &&
                digits(11, 2) && at(13, ':') && digits(14, 2) && at(16, ':') && digits(17, 2))) return null
        var i = 19
        if (at(i, '.') && digits(i + 1, 1)) {
            i++
            while (i < b.length && b[i] in '0'..'9') i++
        }
        if (at(i, 'Z')) {
            i++
        } else if ((at(i, '+') || at(i, '-')) && digits(i + 1, 2) && at(i + 3, ':') && digits(i + 4, 2)) {
            i += 6
        } else {
            return null
        }
        if (!at(i, ' ')) return null
        return b.substring(11, 19) to i + 1
    }

    private val prefixes = listOf("##[error]" to "error", "##[warning]" to "warning", "##[notice]" to "comment", "##[debug]" to "comment")

    private fun trimSpaces(s: String): String = s.trim { it == ' ' || it == '\t' }

    /** Raw log lines (+ per-section `{status, duration}`) → numbered content lines and their `##[group]` sections (contract §3). */
    fun model(lines: Any?, sections: Any? = null): Model {
        val out = ArrayList<Line>()
        val secs = ArrayList<Section>()
        var open = -1
        var closedLast = true
        val metas = sections as? List<*> ?: emptyList<Any?>()
        for (raw in lines as? List<*> ?: emptyList<Any?>()) {
            var text = raw as? String ?: ""
            var time: String? = null
            timestamp(text)?.let { (t, len) ->
                time = t
                text = text.substring(len)
            }
            val plain = stripAnsi(text)
            if (plain.startsWith("##[group]")) {
                if (open >= 0) secs[open].end = out.size
                open = secs.size
                closedLast = false
                secs.add(Section(open, trimSpaces(plain.substring("##[group]".length)), out.size + 1, out.size, 0, "passed", null))
                continue
            }
            if (plain.startsWith("##[endgroup]")) {
                if (open >= 0) {
                    secs[open].end = out.size
                    open = -1
                    closedLast = true
                }
                continue
            }
            var kind: String? = null
            for ((prefix, k) in prefixes) {
                if (plain.startsWith(prefix)) {
                    kind = k
                    text = cutVisible(text, prefix.length)
                    break
                }
            }
            out.add(Line(out.size + 1, open, time, kind, text))
        }
        if (open >= 0) secs[open].end = out.size
        val last = secs.size - 1
        for (i in secs.indices) {
            val s = secs[i]
            s.count = s.end - s.start + 1
            val given = metas.getOrNull(i) as? Map<*, *>
            val st = given?.get("status")
            s.status = if (st != null) status(st) else if (!closedLast && i == last) "running" else "passed"
            s.duration = given?.get("duration") as? String
        }
        return Model(out, secs)
    }

    /** The sections that open on their own: `failed` or `running`. */
    fun openDefault(model: Model): List<Int> = model.sections.filter { it.status == "failed" || it.status == "running" }.map { it.index }

    /** A JSON number (never a bool), else null. */
    fun number(v: Any?): Double? = (v as? Number)?.toDouble()

    /** The section indexes a JSON `open` array names (anything else ignored). */
    fun openSet(open: Any?, model: Model): List<Int> {
        val values = (open as? List<*> ?: emptyList<Any?>()).mapNotNull(::number)
        return model.sections.map { it.index }.filter { values.contains(it.toDouble()) }
    }

    /** The current open sections: a given array, else the default. */
    fun resolvedOpen(open: Any?, model: Model): List<Int> = if (open is List<*>) openSet(open, model) else openDefault(model)

    /** One flat row of a LogLines body: a content line ([n]) or a section head ([section]). */
    sealed interface Row {
        data class LineRow(val n: Int) : Row
        data class SectionRow(val section: Int) : Row
    }

    /** The flat rows a LogLines body windows: lines outside sections, each section's head row, then its lines when its index is in [open]. */
    fun rows(model: Model, open: List<Int>): List<Row> {
        val rows = ArrayList<Row>(model.lines.size + model.sections.size)
        val lines = model.lines
        var line = 0
        val shownSet = open.toHashSet()
        for (s in model.sections) {
            while (line < lines.size && lines[line].n < s.start) rows.add(Row.LineRow(lines[line++].n))
            rows.add(Row.SectionRow(s.index))
            val shown = s.index in shownSet
            while (line < lines.size && lines[line].n <= s.end) {
                if (shown) rows.add(Row.LineRow(lines[line].n))
                line++
            }
        }
        while (line < lines.size) rows.add(Row.LineRow(lines[line++].n))
        return rows
    }

    /** One search match: code-point offsets into the escape-free text, end exclusive. */
    data class Match(val line: Int, val start: Int, val end: Int)

    private fun folded(s: String): IntArray = s.codePoints().map { if (it in 0x41..0x5A) it + 0x20 else it }.toArray()

    /** Every non-overlapping match of [query] (ASCII case-insensitive) in every content line's escape-free text, folded sections too. */
    fun search(model: Model, query: Any?): List<Match> {
        val q = query as? String
        if (q.isNullOrEmpty()) return emptyList()
        val needle = folded(q)
        val out = ArrayList<Match>()
        for (line in model.lines) {
            val t = folded(stripAnsi(line.text))
            var i = 0
            while (i + needle.size <= t.size) {
                var eq = true
                for (k in needle.indices) if (t[i + k] != needle[k]) { eq = false; break }
                if (eq) {
                    out.add(Match(line.n, i, i + needle.size))
                    i += needle.size
                } else {
                    i++
                }
            }
        }
        return out
    }

    /** [open] plus the section holding [line] (sorted, unique, integers only). */
    fun reveal(model: Model, open: Any?, line: Int): List<Int> {
        val set = java.util.TreeSet<Int>()
        for (v in (open as? List<*> ?: emptyList<Any?>()).mapNotNull(::number)) {
            if (Math.rint(v) == v && Math.abs(v) < 9_007_199_254_740_992.0) set.add(v.toInt())
        }
        model.line(line)?.let { if (it.section >= 0) set.add(it.section) }
        return set.toList()
    }

    // §6 CodeBlock blame

    /** One line's blame block: [block] = index into `blame` (-1 before the first entry), [first] its first line, [border] the hairline. */
    data class BlameLine(val block: Int, val first: Boolean, val border: Boolean)

    /** Per line (1…[lineCount]) its blame block: entries sorted by `start` (stable), each running to the next entry's start − 1. */
    fun blameBlocks(blame: Any?, lineCount: Int): List<BlameLine> {
        val entries = ArrayList<Pair<Int, Double>>()
        (blame as? List<*>)?.forEachIndexed { index, e ->
            val s = number((e as? Map<*, *>)?.get("start"))
            if (s != null && Math.rint(s) == s && s >= 1) entries.add(index to s)
        }
        val sorted = entries.sortedBy { it.second } // stable
        val out = ArrayList<BlameLine>(maxOf(lineCount, 0))
        var at = -1
        var firstShown = 0
        for (n in 1..lineCount) {
            var first = false
            while (at + 1 < sorted.size && sorted[at + 1].second <= n.toDouble()) {
                at++
                first = true
            }
            if (first && firstShown == 0) firstShown = n
            out.add(BlameLine(if (at >= 0) sorted[at].first else -1, first, first && n != firstShown))
        }
        return out
    }
}
