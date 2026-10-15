package com.exponential.app.domain

// EXP-1324: the CodeBlock's `ansi` language (the Terminal pattern: command
// output), mirrored from the eUI built-in tokenizer
// (`packages/exponential-ui/src/code.ts` `ansiRuns`/`tokenizeCode`, rule in
// `catalog/code.json` `$comment`). The app does not link the eUI core, so only
// this ONE language lives here; AnsiCodeTest replays the `ansi` cases of
// `packages/exponential-ui/fixtures/code-tokens.json`.
//
// Before scanning, every ANSI CSI escape is REMOVED and the text split into
// runs by SGR colour (31/91 error, 32/92 success, 33/93 warning, 2/90
// comment, 0/39/none reset, anything else keeps the colour; params apply left
// to right). A coloured run is that kind verbatim; an uncoloured run is
// scanned with the `ansi` spec, which has only `lineKinds`: at a line start
// (after spaces/tabs) the LONGEST listed prefix makes the rest of the line
// that kind, everything else is plain. Tokens split into lines and adjacent
// tokens of one kind merge.

/** One token: a contract `codeToken` kind (`plain`, `comment`, `success`, …). */
data class CodeToken(val kind: String, val text: String)

object AnsiCode {
    /** `catalog/code.json` `languages.ansi.lineKinds`. */
    val LINE_KINDS: List<Pair<String, String>> = listOf(
        "$ " to "comment",
        "> " to "comment",
        "BUILD SUCCESSFUL" to "success",
        "BUILD FAILED" to "error",
        "PASS " to "success",
        "FAIL " to "error",
        "FAILED" to "error",
        "(pass)" to "success",
        "(fail)" to "error",
        "test result: ok" to "success",
        "test result: FAILED" to "error",
        "error:" to "error",
        "error[" to "error",
        "Error:" to "error",
        "warning:" to "warning",
        "✓" to "success",
        "✔" to "success",
        "✗" to "error",
        "✘" to "error",
    )

    /** One run of the escape-free text with the SGR colour it was printed in. */
    data class Run(val kind: String?, val text: String)

    private fun sgrKind(params: String, kind: String?): String? {
        var out = kind
        for (raw in params.split(';')) {
            when (if (raw.isEmpty()) 0 else raw.toIntOrNull() ?: -1) {
                0, 39 -> out = null
                31, 91 -> out = "error"
                32, 92 -> out = "success"
                33, 93 -> out = "warning"
                2, 90 -> out = "comment"
            }
        }
        return out
    }

    /** The source with every CSI escape removed, split into runs by colour.
     *  Adjacent runs of one colour merge; an incomplete escape stays text. */
    fun runs(source: String): List<Run> {
        val runs = mutableListOf<Run>()
        var kind: String? = null
        val text = StringBuilder()
        fun flush() {
            if (text.isEmpty()) return
            val last = runs.lastOrNull()
            if (last != null && last.kind == kind) {
                runs[runs.size - 1] = Run(kind, last.text + text)
            } else {
                runs += Run(kind, text.toString())
            }
            text.clear()
        }
        val n = source.length
        var i = 0
        while (i < n) {
            if (source[i] == '\u001b' && i + 1 < n && source[i + 1] == '[') {
                var j = i + 2
                while (j < n && source[j].code in 0x30..0x3F) j++
                val params = source.substring(i + 2, j)
                while (j < n && source[j].code in 0x20..0x2F) j++
                if (j < n && source[j].code in 0x40..0x7E) {
                    if (source[j] == 'm') {
                        val next = sgrKind(params, kind)
                        if (next != kind) {
                            flush()
                            kind = next
                        }
                    }
                    i = j + 1
                    continue
                }
            }
            text.append(source[i])
            i++
        }
        flush()
        return runs
    }

    private fun normalize(source: String): String = source.replace("\r\n", "\n").replace('\r', '\n')

    /** The text the block shows and copies: CRLF → LF, escapes removed. */
    fun text(source: String): String = runs(normalize(source)).joinToString("") { it.text }

    private fun isSpace(c: Char) = c == ' ' || c == '\t'

    /** An uncoloured run through the `ansi` scanner. */
    private fun scan(code: String, lineStart: Boolean): List<CodeToken> {
        val out = mutableListOf<CodeToken>()
        code.split('\n').forEachIndexed { index, line ->
            if (index > 0) out += CodeToken("plain", "\n")
            val atStart = index > 0 || lineStart
            var k = 0
            while (k < line.length && isSpace(line[k])) k++
            val rest = line.substring(k)
            val hit = if (atStart && rest.isNotEmpty()) {
                LINE_KINDS.filter { rest.startsWith(it.first) }.maxByOrNull { it.first.length }
            } else {
                null
            }
            if (hit != null) {
                if (k > 0) out += CodeToken("plain", line.substring(0, k))
                out += CodeToken(hit.second, rest)
            } else if (line.isNotEmpty()) {
                out += CodeToken("plain", line)
            }
        }
        return out
    }

    /** The source tokenized per LINE: each line's tokens concatenate to its
     *  escape-free text; an empty line is an empty list. */
    fun tokenize(source: String): List<List<CodeToken>> {
        val flat = mutableListOf<CodeToken>()
        var atLine = true
        for (run in runs(normalize(source))) {
            if (run.kind != null) {
                flat += CodeToken(run.kind, run.text)
            } else {
                flat += scan(run.text, atLine)
            }
            val tail = run.text.split('\n').last()
            val onlySpaces = tail.all(::isSpace)
            atLine = if ('\n' in run.text) onlySpaces else atLine && onlySpaces
        }
        val lines = mutableListOf(mutableListOf<CodeToken>())
        for (token in flat) {
            token.text.split('\n').forEachIndexed { index, piece ->
                if (index > 0) lines += mutableListOf<CodeToken>()
                if (piece.isEmpty()) return@forEachIndexed
                val line = lines.last()
                val last = line.lastOrNull()
                if (last != null && last.kind == token.kind) {
                    line[line.size - 1] = CodeToken(last.kind, last.text + piece)
                } else {
                    line += CodeToken(token.kind, piece)
                }
            }
        }
        return lines
    }
}
