package at.exponential.ui.primitives

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/**
 * #VAPP-258: fixtures/markdown-render.json, the golden render fixture: this
 * parser's blocks in the shared normal form. A pending case must still
 * differ (the list only shrinks).
 */
class MarkdownRenderTest {
    private val sdk: File = File("../../exponential-ui")

    @Suppress("UNCHECKED_CAST")
    private fun fixture(): Map<String, Any?> {
        val file = File(sdk, "fixtures/markdown-render.json")
        assumeTrue("$file not in this checkout", file.isFile)
        return MiniJson(file.readText()).parse() as Map<String, Any?>
    }

    private fun runs(inlines: List<MarkdownInline>): List<Map<String, Any?>> {
        val out = mutableListOf<MutableMap<String, Any?>>()
        var lastKey: String? = null
        for (run in inlines) {
            if (run.text.isEmpty()) continue
            val key = "${run.bold}${run.italic}${run.code}${run.strike}${run.link ?: "\u0000"}"
            if (key == lastKey) {
                out.last()["text"] = (out.last()["text"] as String) + run.text
                continue
            }
            val r = linkedMapOf<String, Any?>("text" to run.text)
            if (run.bold) r["bold"] = true
            if (run.italic) r["italic"] = true
            if (run.code) r["code"] = true
            if (run.strike) r["strike"] = true
            run.link?.let { r["link"] = it }
            out += r
            lastKey = key
        }
        return out
    }

    private fun normal(blocks: List<MarkdownBlock>): List<Map<String, Any?>> = blocks.map { b ->
        when (val k = b.kind) {
            MarkdownBlockKind.Paragraph -> mapOf("kind" to "paragraph", "runs" to runs(b.inlines))
            MarkdownBlockKind.Quote -> mapOf("kind" to "quote", "runs" to runs(b.inlines))
            is MarkdownBlockKind.Heading -> mapOf("kind" to "heading", "level" to k.level, "runs" to runs(b.inlines))
            is MarkdownBlockKind.ListItem -> buildMap {
                put("kind", "listItem")
                put("marker", k.marker)
                k.task?.let { put("task", it) }
                put("depth", k.depth)
                put("runs", runs(b.inlines))
            }
            MarkdownBlockKind.CodeBlock -> buildMap {
                put("kind", "codeBlock")
                b.lang?.let { put("lang", it) }
                put("text", b.inlines.joinToString("") { it.text })
            }
            is MarkdownBlockKind.Table -> mapOf("kind" to "table", "header" to k.header.map(::runs), "rows" to k.rows.map { row -> row.map(::runs) })
            MarkdownBlockKind.Rule -> mapOf("kind" to "rule")
            is MarkdownBlockKind.Image -> mapOf("kind" to "image", "src" to k.src, "alt" to k.alt)
            is MarkdownBlockKind.Attachment -> mapOf("kind" to "attachment", "type" to k.type, "name" to k.name, "href" to k.href)
        }
    }

    /** Key-order-free, number-type-free text of a JSON value. */
    private fun canon(v: Any?): String = when (v) {
        null -> "null"
        is Map<*, *> -> v.entries.sortedBy { it.key as String }.joinToString(",", "{", "}") { "${it.key}:${canon(it.value)}" }
        is List<*> -> v.joinToString(",", "[", "]") { canon(it) }
        is Number -> v.toDouble().toLong().toString()
        is String -> "\"" + v + "\""
        else -> v.toString()
    }

    @Test
    @Suppress("UNCHECKED_CAST")
    fun everyCaseMatchesTheNormalForm() {
        val f = fixture()
        val pending = ((f["pending"] as Map<String, Any?>)["compose"] as List<String>?).orEmpty().toSet()
        val cases = f["cases"] as List<Map<String, Any?>>
        assertTrue(cases.size > 50)
        val failures = mutableListOf<String>()
        for (c in cases) {
            val name = c["name"] as String
            val actual = canon(normal(Markdown.parse(c["text"] as String)))
            val expected = canon(c["blocks"])
            if (name in pending) {
                if (actual == expected) failures += "$name passes now: drop it from pending.compose"
            } else if (actual != expected) {
                failures += "$name\n  expected $expected\n  actual   $actual"
            }
        }
        assertEquals("", failures.joinToString("\n"))
        assertNotEquals(0, cases.size)
    }

    /** #VAPP-258 step 5: host options (`breaks`). */
    @Test
    @Suppress("UNCHECKED_CAST")
    fun optionCases() {
        val f = fixture()
        val failures = mutableListOf<String>()
        for (c in f["optionCases"] as List<Map<String, Any?>>) {
            val hard = (c["options"] as Map<String, Any?>?)?.get("breaks") == "hard"
            val actual = canon(normal(Markdown.parse(c["text"] as String, hard)))
            if (actual != canon(c["blocks"])) failures += "${c["name"]}\n  expected ${canon(c["blocks"])}\n  actual   $actual"
        }
        assertEquals("", failures.joinToString("\n"))
    }

    /** #VAPP-258 step 5: documents streamed in prefix by prefix, in order (the memo) and cold. */
    @Test
    @Suppress("UNCHECKED_CAST")
    fun streamingCases() {
        val f = fixture()
        val cases = f["streamingCases"] as List<Map<String, Any?>>
        assertTrue(cases.isNotEmpty())
        val failures = mutableListOf<String>()
        for (c in cases) {
            val text = c["text"] as String
            val hard = (c["options"] as Map<String, Any?>?)?.get("breaks") == "hard"
            for (cold in listOf(false, true)) {
                Markdown.resetStreamMemo()
                for ((i, expected) in (c["steps"] as List<Any?>).withIndex()) {
                    if (cold) Markdown.resetStreamMemo()
                    val prefix = text.substring(0, i + 1)
                    val actual = canon(normal(Markdown.parse(prefix, hard, streaming = true)))
                    if (actual != canon(expected)) {
                        failures += "${c["name"]} \"$prefix\" cold=$cold\n  expected ${canon(expected)}\n  actual   $actual"
                        break
                    }
                }
            }
        }
        assertEquals("", failures.joinToString("\n"))
    }

    /** A streamed text's later list is its own group (a gap between lists). */
    @Test
    fun streamedListsKeepTheirGroups() {
        Markdown.resetStreamMemo()
        Markdown.parse("- a\n- b\n\n", false, streaming = true)
        val blocks = Markdown.parse("- a\n- b\n\n- c", false, streaming = true)
        assertEquals(blocks[0].listGroup, blocks[1].listGroup)
        assertNotEquals(blocks[1].listGroup, blocks[2].listGroup)
    }
}
