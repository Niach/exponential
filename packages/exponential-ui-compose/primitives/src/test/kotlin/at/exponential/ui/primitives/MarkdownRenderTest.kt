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
}
