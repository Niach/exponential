package com.exponential.app.domain

/**
 * EXP-895 — the ONE diff model every client renders, and the ONE parser that
 * builds it. Hand-mirrored ×4 (TS `@exp/domain-contract` `diff.ts`, desktop
 * `domain::diff`, iOS `ExpCore/Sources/Domain/Diff.swift`, this file) and
 * byte-locked by `fixtures/diff/cases.json` + `fixtures/diff/summary.json`,
 * which every client's test replays.
 *
 * Three producers feed the same model:
 *   1. `git diff` output from the desktop worktree (`diff --git` sections),
 *   2. the steer relay's per-call tool diffs (bare `--- a/x` / `+++ b/x`
 *      sections with no `diff --git` line, optionally cut with a trailing
 *      `\ N more lines truncated` marker),
 *   3. GitHub's PullFile `patch` (hunks only; the path and status arrive
 *      beside it, never inside it) — [parsePatch] / [Status.fromPullFile].
 *
 * The parse rules ARE the contract; a change here is a change on all four
 * clients. The projection [render] is what the fixture freezes, so every
 * platform can compare one list of strings instead of a whole object graph.
 */
object Diff {

    // ── Model ───────────────────────────────────────────────────────────────

    enum class Status(val wire: String) {
        ADDED("added"),
        REMOVED("removed"),
        MODIFIED("modified"),
        RENAMED("renamed"),
        COPIED("copied");

        companion object {
            /**
             * GitHub's file status vocabulary → ours. `changed`, `unchanged`
             * and anything a future API adds read as [MODIFIED].
             */
            fun fromPullFile(raw: String): Status = when (raw) {
                "added" -> ADDED
                "removed" -> REMOVED
                "modified" -> MODIFIED
                "renamed" -> RENAMED
                "copied" -> COPIED
                else -> MODIFIED
            }
        }
    }

    enum class LineKind { ADD, DEL, CONTEXT, META }

    data class Line(
        val kind: LineKind,
        /** 1-based line number on the old side; absent on `add`/`meta`. */
        val oldNo: Int? = null,
        /** 1-based line number on the new side; absent on `del`/`meta`. */
        val newNo: Int? = null,
        /** The line's content, its one-character sign stripped. */
        val text: String,
    )

    data class Hunk(
        val oldStart: Int,
        val oldLines: Int,
        val newStart: Int,
        val newLines: Int,
        /** The verbatim `@@ … @@` line, section heading and all. */
        val header: String,
        val lines: List<Line>,
    )

    data class File(
        val path: String,
        /** Only on [Status.RENAMED]/[Status.COPIED]: where the file came from. */
        val previousPath: String? = null,
        val status: Status = Status.MODIFIED,
        val additions: Int = 0,
        val deletions: Int = 0,
        val binary: Boolean = false,
        val hunks: List<Hunk> = emptyList(),
    )

    data class Parsed(
        val files: List<File>,
        /**
         * Lines the PUBLISHER dropped, read back off its `\ N more lines
         * truncated` marker (EXP-786). Null when the diff is whole.
         */
        val truncatedLines: Int? = null,
    )

    /**
     * Every line number and count saturates here (i32::MAX) — the natives
     * carry 32-bit counters and a hostile `@@` header must never wrap one.
     */
    const val LINE_MAX: Int = 2147483647

    /**
     * The one marker the steer relay appends to a cut patch (EXP-786); web
     * `splitTruncatedDiff` and desktop `truncated_marker_count` spell it the
     * same way. Anchored to the END of the text: only a TRAILING marker
     * counts (`\z`, never a match before a final line terminator).
     */
    private val TRUNCATION_MARKER = Regex("""(?:^|\n)\\ (\d+) more lines? truncated\s*\z""")

    private val HUNK_HEADER = Regex("""^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@""")

    /** The quoted form of a `diff --git <a> <b>` pair, which parses exactly. */
    private val DIFF_GIT_QUOTED = Regex("""(?:"(?:[^"]*)"|\S+)\s+"([^"]*)"""")

    // Path precedence: a higher-ranked source overwrites a lower-ranked one,
    // and never the other way round. `rename to`/`copy to` name the
    // destination outright, `+++` is the new side, `---` the old side (a
    // fallback for a diff that never reaches its `+++`), `diff --git`'s
    // b-side is the last resort because a path with a space makes that line
    // ambiguous.
    private const val RANK_NONE = -1
    private const val RANK_DIFF_GIT = 0
    private const val RANK_OLD = 1
    private const val RANK_NEW = 2
    private const val RANK_RENAME = 3

    /** A file under construction: [File] plus the rank of its current path. */
    private class Building(
        var path: String = "",
        var status: Status = Status.MODIFIED,
    ) {
        var previousPath: String? = null
        var additions: Int = 0
        var deletions: Int = 0
        var binary: Boolean = false
        val hunks: MutableList<MutableHunk> = mutableListOf()
        var pathRank: Int = RANK_NONE

        fun seal(): File = File(
            path = path,
            previousPath = previousPath,
            status = status,
            additions = additions,
            deletions = deletions,
            binary = binary,
            hunks = hunks.map { it.seal() },
        )
    }

    private class MutableHunk(
        val oldStart: Int,
        val oldLines: Int,
        val newStart: Int,
        val newLines: Int,
        val header: String,
    ) {
        val lines: MutableList<Line> = mutableListOf()

        fun seal(): Hunk = Hunk(oldStart, oldLines, newStart, newLines, header, lines.toList())
    }

    /**
     * A run of decimal digits → a count, saturating at [LINE_MAX] instead of
     * throwing or wrapping (a `@@` header may name any number at all).
     */
    private fun parseCount(digits: String): Int {
        var n = 0L
        for (ch in digits) {
            if (ch !in '0'..'9') return 0
            n = n * 10 + (ch - '0')
            if (n >= LINE_MAX) return LINE_MAX
        }
        return n.toInt()
    }

    /** Sum two counters without wrapping. */
    private fun clampLong(n: Long): Int = when {
        n <= 0L -> 0
        n >= LINE_MAX.toLong() -> LINE_MAX
        else -> n.toInt()
    }

    /** Advance a 1-based line counter, saturating rather than wrapping. */
    private fun step(n: Int): Int = if (n >= LINE_MAX) LINE_MAX else n + 1

    /**
     * A `---`/`+++`/`diff --git` payload → a display path: drop ONE trailing
     * `\r` (a CRLF-framed patch, split on `\n` alone), cut at the first
     * TAB (GNU diff's timestamp column), then unwrap surrounding double
     * quotes (git quotes a path carrying control or non-ASCII bytes). The
     * `a/`/`b/` prefix is stripped by [stripAb] — only where git actually
     * writes one.
     */
    private fun cutPath(raw: String): String {
        val line = raw.removeSuffix("\r")
        val tab = line.indexOf('\t')
        var s = if (tab >= 0) line.substring(0, tab) else line
        if (s.length >= 2 && s.startsWith("\"") && s.endsWith("\"")) s = s.substring(1, s.length - 1)
        return s
    }

    /**
     * Drop the one `a/`/`b/` prefix git puts on `---`, `+++` and `diff --git`
     * paths. NOT applied to `rename from`/`rename to`/`copy from`/`copy to`,
     * which git writes bare — stripping there would eat a real top-level
     * `a/` directory.
     */
    private fun stripAb(s: String): String =
        if (s.startsWith("a/") || s.startsWith("b/")) s.substring(2) else s

    /**
     * The b-side of a `diff --git <a> <b>` line. Quoted pairs parse exactly;
     * otherwise the last ` b/` wins (git's own ambiguity — an unquoted path
     * with a space cannot be split reliably, which is why this is the lowest
     * rank).
     */
    private fun diffGitNewPath(rest: String): String? {
        val quoted = DIFF_GIT_QUOTED.matchEntire(rest)
        if (quoted != null) return stripAb(quoted.groupValues[1])
        val at = rest.lastIndexOf(" b/")
        if (at >= 0) return rest.substring(at + 3)
        val trimmed = rest.trim()
        return if (trimmed.isNotEmpty()) stripAb(trimmed) else null
    }

    private fun parseHunkHeader(line: String): MutableHunk? {
        val m = HUNK_HEADER.find(line) ?: return null
        val groups = m.groupValues
        return MutableHunk(
            oldStart = parseCount(groups[1]),
            // A count the header omits is 1 — `@@ -1 +1 @@` is one line each side.
            oldLines = if (groups[2].isEmpty()) 1 else parseCount(groups[2]),
            newStart = parseCount(groups[3]),
            newLines = if (groups[4].isEmpty()) 1 else parseCount(groups[4]),
            header = line,
        )
    }

    // ── The parser ──────────────────────────────────────────────────────────

    /**
     * The one state machine behind [parse] and [parsePatch].
     *
     * `seedPath`/`seedStatus` = the caller already knows the path and status
     * (a GitHub patch), so every header line is ignored (except the binary
     * marker) and no `---`/`diff --git` line may ever open a second file.
     */
    private fun parseSections(
        text: String,
        seedPath: String? = null,
        seedStatus: Status = Status.MODIFIED,
    ): List<File> {
        val files = mutableListOf<File>()
        val seeded = seedPath != null
        var cur: Building? = if (seedPath != null) Building(seedPath, seedStatus) else null
        var hunk: MutableHunk? = null
        var remOld = 0
        var remNew = 0
        var oldNo = 0
        var newNo = 0

        fun setPath(file: Building, path: String, rank: Int) {
            if (rank >= file.pathRank) {
                file.path = path
                file.pathRank = rank
            }
        }

        val lines = text.split("\n")
        for ((i, raw) in lines.withIndex()) {
            // 1. `diff --git` starts the next file unconditionally — even
            //    mid-hunk, where a truncated patch can leave us.
            if (!seeded && raw.startsWith("diff --git ")) {
                val next = Building()
                val path = diffGitNewPath(raw.substring("diff --git ".length))
                if (path != null) setPath(next, path, RANK_DIFF_GIT)
                cur?.let { files.add(it.seal()) }
                cur = next
                hunk = null
                remOld = 0
                remNew = 0
                continue
            }

            // 2. A hunk header. Body lines always carry a sign, so a line
            //    literally starting with `@@` is unambiguous.
            if (raw.startsWith("@@")) {
                // A `@@` line, and a new file, always end the hunk in progress.
                hunk = null
                remOld = 0
                remNew = 0
                val head = parseHunkHeader(raw) ?: continue
                val file = cur ?: Building().also { cur = it }
                file.hunks.add(head)
                hunk = head
                remOld = head.oldLines
                remNew = head.newLines
                oldNo = head.oldStart
                newNo = head.newStart
                continue
            }

            // 3. `\ No newline at end of file`: unified-diff metadata. Kept as
            //    a row (a reader wants to see it), numbered on neither side,
            //    and it never consumes a count — so it may legally trail a
            //    hunk whose counts are already spent, as it does when BOTH
            //    sides lack the final newline.
            val open = hunk
            if (open != null && raw.startsWith("\\")) {
                open.lines.add(
                    Line(
                        kind = LineKind.META,
                        text = if (raw.startsWith("\\ ")) raw.substring(2) else raw.substring(1),
                    )
                )
                continue
            }

            // 4. The hunk body, bounded by the header's counts. Past them the
            //    hunk is over, whatever the next line looks like — that is
            //    what lets a bare steer diff start its next file on a plain
            //    `--- a/…`. Inside them a `--- ` line with a `+++ ` line
            //    right behind it (one-line lookahead) is STILL the next bare
            //    section's opener, not a deletion of `-- …`: a header whose
            //    counts overshoot its body must not swallow the file after
            //    it. A `---` body line followed by anything else stays a
            //    deletion.
            val bareOpener = !seeded &&
                raw.startsWith("--- ") &&
                lines.getOrNull(i + 1)?.startsWith("+++ ") == true
            if (open != null && (remOld > 0 || remNew > 0) && !bareOpener) {
                // An open hunk always belongs to a file: rule 2 opens one.
                val file = cur!!
                val sign = raw.firstOrNull()
                if (sign == '+') {
                    file.additions = clampLong(file.additions.toLong() + 1L)
                    open.lines.add(Line(kind = LineKind.ADD, newNo = newNo, text = raw.substring(1)))
                    newNo = step(newNo)
                    remNew -= 1
                    continue
                }
                if (sign == '-') {
                    file.deletions = clampLong(file.deletions.toLong() + 1L)
                    open.lines.add(Line(kind = LineKind.DEL, oldNo = oldNo, text = raw.substring(1)))
                    oldNo = step(oldNo)
                    remOld -= 1
                    continue
                }
                // A context line is ` ` + content; a producer that trimmed
                // trailing whitespace emits the empty string for an empty
                // context line, and inside a hunk with counts left that is
                // exactly what it means.
                if (sign == ' ' || raw.isEmpty()) {
                    open.lines.add(
                        Line(
                            kind = LineKind.CONTEXT,
                            oldNo = oldNo,
                            newNo = newNo,
                            text = if (raw.isEmpty()) "" else raw.substring(1),
                        )
                    )
                    oldNo = step(oldNo)
                    newNo = step(newNo)
                    remOld -= 1
                    remNew -= 1
                    continue
                }
                // Anything else inside a hunk means the counts lied: end the
                // hunk and let the line be read as a header below.
                hunk = null
                remOld = 0
                remNew = 0
            }

            if (seeded) {
                val file = cur!!
                if (
                    file.hunks.isEmpty() &&
                    (raw.startsWith("Binary files ") || raw.startsWith("GIT binary patch"))
                ) {
                    file.binary = true
                }
                continue
            }

            // 5. The header region. `---` is the one header line that may also
            //    OPEN a file: a bare steer section has no `diff --git` to
            //    announce it.
            if (raw.startsWith("--- ") || raw == "---") {
                val previous = cur
                val file = if (previous != null && previous.hunks.isEmpty()) previous else {
                    previous?.let { files.add(it.seal()) }
                    hunk = null
                    remOld = 0
                    remNew = 0
                    Building().also { cur = it }
                }
                val payload = cutPath(if (raw.length > 4) raw.substring(4) else "")
                if (payload == "/dev/null") file.status = Status.ADDED
                else if (payload.isNotEmpty()) setPath(file, stripAb(payload), RANK_OLD)
                continue
            }
            val file = cur ?: continue
            // Everything below is honoured only BEFORE the first hunk of a
            // file — past it these words are just content that lost its sign.
            if (file.hunks.isNotEmpty()) continue
            when {
                raw.startsWith("+++ ") -> {
                    val payload = cutPath(raw.substring(4))
                    if (payload == "/dev/null") file.status = Status.REMOVED
                    else if (payload.isNotEmpty()) setPath(file, stripAb(payload), RANK_NEW)
                }
                raw.startsWith("new file mode") -> file.status = Status.ADDED
                raw.startsWith("deleted file mode") -> file.status = Status.REMOVED
                raw.startsWith("rename from ") -> {
                    file.previousPath = cutPath(raw.substring("rename from ".length))
                    file.status = Status.RENAMED
                }
                raw.startsWith("rename to ") -> {
                    setPath(file, cutPath(raw.substring("rename to ".length)), RANK_RENAME)
                    file.status = Status.RENAMED
                }
                raw.startsWith("copy from ") -> {
                    file.previousPath = cutPath(raw.substring("copy from ".length))
                    file.status = Status.COPIED
                }
                raw.startsWith("copy to ") -> {
                    setPath(file, cutPath(raw.substring("copy to ".length)), RANK_RENAME)
                    file.status = Status.COPIED
                }
                raw.startsWith("Binary files ") || raw.startsWith("GIT binary patch") ->
                    file.binary = true
            }
        }

        cur?.let { files.add(it.seal()) }
        return files
    }

    /**
     * Read any of the three forms into the model, auto-detected:
     *
     * - a full `git diff` (`diff --git` sections),
     * - bare steer sections that start straight at `--- a/x` / `+++ b/x`,
     * - hunks-only text (the first non-blank line is a `@@` header) → ONE
     *   file with an EMPTY path and status `modified`; a caller that knows
     *   the path uses [parsePatch] instead.
     *
     * Garbage, whitespace and the empty string all yield no files.
     */
    fun parse(text: String): Parsed {
        if (text.isEmpty()) return Parsed(emptyList())
        var body = text
        var truncatedLines: Int? = null
        val cut = TRUNCATION_MARKER.find(body)
        if (cut != null) {
            body = body.substring(0, cut.range.first)
            truncatedLines = parseCount(cut.groupValues[1])
        }
        val files = parseSections(body).filter { file ->
            // A section that named neither a path nor a hunk is noise, not a file.
            file.path.isNotEmpty() || file.hunks.isNotEmpty() || file.binary
        }
        if (files.isEmpty()) return Parsed(emptyList())
        return Parsed(files, truncatedLines)
    }

    /**
     * A hunks-only patch whose path and status the CALLER knows (GitHub's
     * PullFile, the desktop's per-file `git diff` wrappers). Nothing in
     * [patch] may change either one. A missing or empty patch is a file with
     * no hunks — binary, too large for GitHub to send, or a pure rename.
     */
    fun parsePatch(path: String, status: Status, patch: String?): File {
        if (patch.isNullOrEmpty()) return File(path = path, status = status)
        return parseSections(patch, seedPath = path, seedStatus = status).first()
    }

    /**
     * One GitHub PullFile → one [File] (web `fromPullFile`; the fixture's
     * `pullFile` form). An empty [previousFilename] is no previous path. When
     * the patch carries no hunks (absent, empty, or a pure rename) GitHub's own
     * counts are kept, clamped to `0..LINE_MAX` — they are the only counts
     * there are.
     */
    fun fromPullFile(
        filename: String,
        previousFilename: String?,
        status: String,
        additions: Long,
        deletions: Long,
        patch: String?,
    ): File {
        val file = parsePatch(filename, Status.fromPullFile(status), patch)
            .let { if (previousFilename.isNullOrEmpty()) it else it.copy(previousPath = previousFilename) }
        if (file.hunks.isNotEmpty()) return file
        return file.copy(additions = clampCount(additions), deletions = clampCount(deletions))
    }

    private fun clampCount(n: Long): Int = n.coerceIn(0L, LINE_MAX.toLong()).toInt()

    // ── Derivations ─────────────────────────────────────────────────────────

    data class Totals(val files: Int, val additions: Int, val deletions: Int)

    fun totals(files: List<File>): Totals {
        var additions = 0L
        var deletions = 0L
        for (file in files) {
            additions += file.additions.toLong()
            deletions += file.deletions.toLong()
        }
        return Totals(files.size, clampLong(additions), clampLong(deletions))
    }

    /**
     * Fold sections that name the SAME path into one file, in order of first
     * appearance. A publisher may emit one section per edit, so the same file
     * arrives several times in one transcript; the reader wants one card.
     * Hunks concatenate in arrival order, counts sum, and the LATER section's
     * status, binary flag and (when it has one) previousPath win.
     */
    fun mergeFilesByPath(files: List<File>): List<File> {
        val out = mutableListOf<File>()
        val at = mutableMapOf<String, Int>()
        for (file in files) {
            val seen = at[file.path]
            if (seen == null) {
                at[file.path] = out.size
                out.add(file)
                continue
            }
            val target = out[seen]
            out[seen] = target.copy(
                hunks = target.hunks + file.hunks,
                additions = clampLong(target.additions.toLong() + file.additions.toLong()),
                deletions = clampLong(target.deletions.toLong() + file.deletions.toLong()),
                status = file.status,
                binary = file.binary,
                previousPath = if (file.previousPath.isNullOrEmpty()) target.previousPath
                else file.previousPath,
            )
        }
        return out
    }

    /**
     * Unchanged lines above a file's FIRST hunk — the count a "show more"
     * affordance offers to expand.
     */
    fun unchangedBefore(first: Hunk): Int = maxOf(0, first.newStart - 1)

    /** Unchanged lines between two consecutive hunks of one file. */
    fun unchangedBetween(prev: Hunk, next: Hunk): Int {
        // Long arithmetic: both sides saturate at [LINE_MAX], so their sum
        // overflows a 32-bit counter where the reference does plain doubles.
        val end = prev.newStart.toLong() + prev.newLines.toLong()
        return maxOf(0L, next.newStart.toLong() - end).toInt()
    }

    fun unchangedLabel(n: Int): String = "$n unchanged ${if (n == 1) "line" else "lines"}"

    fun additionsLabel(n: Int): String = "+$n"

    /**
     * U+2212 MINUS SIGN, not a hyphen: the deletion count sits beside `+n` in
     * a proportional font and a hyphen reads a full notch lighter.
     */
    fun deletionsLabel(n: Int): String = "−$n"

    fun summaryLabel(files: Int, additions: Int, deletions: Int): String {
        if (files == 0) return DomainContract.diffUiNoChanges
        val noun = if (files == 1) "file" else "files"
        return "$files $noun ${additionsLabel(additions)} ${deletionsLabel(deletions)}"
    }

    /**
     * The byte-lock projection: one string per row, the whole [Parsed]
     * flattened. `fixtures/diff/cases.json` stores exactly this, so every
     * platform compares a list of strings instead of reimplementing
     * structural equality. Deliberately ASCII (an ASCII `-` for the deletion
     * count, unlike [deletionsLabel]) and deliberately delimited (`|…|`
     * around content) so trailing whitespace in a diff line survives the
     * round trip.
     */
    fun render(parsed: Parsed): List<String> {
        val out = mutableListOf<String>()
        parsed.truncatedLines?.let { out.add("truncated $it") }
        for (file in parsed.files) {
            val from = file.previousPath?.takeIf { it.isNotEmpty() }?.let { " <- $it" } ?: ""
            val binary = if (file.binary) " binary" else ""
            out.add(
                "file ${file.status.wire} ${file.path}$from$binary " +
                    "+${file.additions} -${file.deletions}"
            )
            for (hunk in file.hunks) {
                out.add(
                    "hunk ${hunk.oldStart},${hunk.oldLines} " +
                        "${hunk.newStart},${hunk.newLines} |${hunk.header}|"
                )
                for (line in hunk.lines) {
                    val old = line.oldNo?.toString() ?: "-"
                    val next = line.newNo?.toString() ?: "-"
                    val tag = when (line.kind) {
                        LineKind.CONTEXT -> "ctx"
                        LineKind.ADD -> "add"
                        LineKind.DEL -> "del"
                        LineKind.META -> "meta"
                    }
                    out.add("$tag $old $next |${line.text}|")
                }
            }
        }
        return out
    }
}
