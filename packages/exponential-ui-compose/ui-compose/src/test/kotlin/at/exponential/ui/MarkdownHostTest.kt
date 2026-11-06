package at.exponential.ui

import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostAttachment
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.host.ImageSize
import at.exponential.ui.host.TextLinkChip
import at.exponential.ui.host.TextLinkSegment
import at.exponential.ui.host.UrlPolicy
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownBlock
import at.exponential.ui.primitives.MarkdownBlockKind
import at.exponential.ui.primitives.MarkdownChipStyle
import at.exponential.ui.primitives.MarkdownInline
import at.exponential.ui.primitives.MarkdownOptions
import at.exponential.ui.primitives.MarkdownSize
import at.exponential.ui.primitives.MarkdownHooks
import at.exponential.ui.primitives.MarkdownStyles
import at.exponential.ui.primitives.MarkdownTextMeasure
import at.exponential.ui.primitives.MarkdownTextSpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * fixtures/markdown-markdown-host.json's FAKE HOST: `textLinks` = the first
 * `links` key (in order) at each index, `imageSize` = `images[src]`,
 * `attachment` = `attachments[href]`, the URL policy = `policy`; presses
 * on block images are recorded.
 */
internal class FakeMarkdownHost(f: JsonValue) : HostPlugin {
    private val links: List<Pair<String, JsonValue>> = f["links"]!!.obj!!.entries.map { it.key to it.value }
    private val images = f["images"]!!.obj!!
    private val attachments = f["attachments"]!!.obj!!
    private val policy = f["policy"]!!
    val pressed = ArrayList<String>()
    val urls = ArrayList<String>()

    override val urlPolicy: UrlPolicy = UrlPolicy(baseUrl = policy["baseUrl"]?.string, linkBase = policy["linkBase"]?.string)

    override fun openUrl(url: String) {
        urls.add(url)
    }

    override fun textLinks(text: String): List<TextLinkSegment>? {
        val out = ArrayList<TextLinkSegment>()
        var plain = 0
        var i = 0
        var found = false
        while (i < text.length) {
            val hit = links.firstOrNull { text.startsWith(it.first, i) }
            if (hit == null) {
                i += 1
                continue
            }
            if (i > plain) out.add(TextLinkSegment(text.substring(plain, i)))
            val chip = hit.second["chip"]?.let { TextLinkChip(it["kind"]!!.string!!, it["label"]!!.string!!, it["icon"]?.string, it["ident"]?.string) }
            out.add(TextLinkSegment(hit.first, hit.second["href"]?.string, chip))
            i += hit.first.length
            plain = i
            found = true
        }
        if (!found) return null
        if (plain < text.length) out.add(TextLinkSegment(text.substring(plain)))
        return out
    }

    override fun imageSize(src: String): ImageSize? =
        images[src]?.let { ImageSize(it["width"]!!.number!!, it["height"]!!.number!!) }

    override fun attachment(href: String): HostAttachment? =
        attachments[href]?.let { HostAttachment(it["kind"]!!.string!!, it["name"]!!.string!!) }

    override val onImagePress: ((String) -> Unit) = { pressed.add(it) }
}

/** The normal form of fixtures/markdown-render.json plus markdown-host.json's `chip`, `attachment` and image `width` / `height`. */
internal object MarkdownNormalForm {
    private fun runs(inlines: List<MarkdownInline>): List<Any?> {
        val out = mutableListOf<MutableMap<String, Any?>>()
        var lastKey: String? = null
        for (run in inlines) {
            if (run.text.isEmpty()) continue
            val key = if (run.chip != null) null else "${run.bold}${run.italic}${run.code}${run.strike}${run.link ?: "\u0000"}"
            if (key != null && key == lastKey) {
                out.last()["text"] = (out.last()["text"] as String) + run.text
                continue
            }
            val r = linkedMapOf<String, Any?>("text" to run.text)
            if (run.bold) r["bold"] = true
            if (run.italic) r["italic"] = true
            if (run.code) r["code"] = true
            if (run.strike) r["strike"] = true
            run.link?.let { r["link"] = it }
            run.chip?.let { c -> r["chip"] = buildMap { put("kind", c.kind); put("label", c.label); c.icon?.let { put("icon", it) }; c.shownIdent?.let { put("ident", it) } } }
            out += r
            lastKey = key
        }
        return out
    }

    fun of(blocks: List<MarkdownBlock>): List<Any?> = blocks.map { b ->
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
            is MarkdownBlockKind.Image -> buildMap {
                put("kind", "image")
                put("src", k.src)
                put("alt", k.alt)
                k.width?.let { put("width", it) }
                k.height?.let { put("height", it) }
            }
            is MarkdownBlockKind.Attachment -> mapOf("kind" to "attachment", "type" to k.type, "name" to k.name, "href" to k.href)
        }
    }

    /** Key-order-free, number-type-free text of a Kotlin or [JsonValue] tree. */
    fun canon(v: Any?): String = when (v) {
        null, JsonValue.Null -> "null"
        is JsonValue.Obj -> canon(v.v)
        is JsonValue.Arr -> canon(v.v)
        is JsonValue.Str -> canon(v.v)
        is JsonValue.Num -> canon(v.v)
        is JsonValue.Bool -> v.v.toString()
        is Map<*, *> -> v.entries.sortedBy { it.key as String }.joinToString(",", "{", "}") { "${it.key}:${canon(it.value)}" }
        is List<*> -> v.joinToString(",", "[", "]") { canon(it) }
        is Number -> "%.2f".format(java.util.Locale.ROOT, v.toDouble())
        is String -> "\"" + v + "\""
        else -> v.toString()
    }
}

/**
 * #VAPP-258 (catalog/host.json `markdown`, fixtures/markdown-host.json): the
 * host hooks through THE pipeline the measurer and the painter share
 * (`SurfaceModel.markdownBlocks`). Plain JVM.
 */
class MarkdownHostTest {
    private fun fixture(): JsonValue {
        Fixtures.require()
        return Fixtures.json("markdown-host.json")
    }

    private fun model(host: HostPlugin) = SurfaceModel("markdown-host", host = host, scope = CoroutineScope(Dispatchers.Unconfined), shaper = noShaper)

    @Test
    fun everyCaseMatchesTheNormalForm() {
        val f = fixture()
        val m = model(FakeMarkdownHost(f))
        val cases = f["cases"]!!.array!!
        assertTrue(cases.size >= 20)
        val failures = ArrayList<String>()
        for (c in cases) {
            val actual = MarkdownNormalForm.canon(MarkdownNormalForm.of(m.markdownBlocks(c["text"]!!.string!!)))
            val expected = MarkdownNormalForm.canon(c["blocks"])
            if (actual != expected) failures += "${c["name"]!!.string}\n  expected $expected\n  actual   $actual"
        }
        assertEquals("", failures.joinToString("\n"))
    }

    @Test
    fun everyImageBoxMatches() {
        val f = fixture()
        val boxes = f["imageBoxes"]!!.array!!
        assertTrue(boxes.isNotEmpty())
        for (b in boxes) {
            fun sizeOf(v: JsonValue?) = v?.takeIf { it != JsonValue.Null }?.let { MarkdownSize(it["width"]!!.number!!.toFloat(), it["height"]!!.number!!.toFloat()) }
            // `loaded`: an unknown (or invalid) host size takes the decoded one.
            val size = Markdown.imageSizeOf(sizeOf(b["size"]), sizeOf(b["loaded"]))
            // `maxHeight`: absent = 160, null = none; `viewport`: absent = unknown.
            val cap = if (b.obj!!.containsKey("maxHeight")) b["maxHeight"]?.number?.toFloat() else 160f
            val box = Markdown.imageBox(size?.width, size?.height, b["available"]!!.number!!.toFloat(), cap, b["viewport"]?.number?.toFloat())
            val name = b["name"]!!.string
            assertEquals(name, b["box"]!!["width"]!!.number!!.toFloat(), box.width, 0.01f)
            assertEquals(name, b["box"]!!["height"]!!.number!!.toFloat(), box.height, 0.01f)
        }
    }

    /** #VAPP-258 (`loaded`): the media cache sizes only the images the host could not size. */
    @Test
    fun loadedSizesFillOnlyUnsizedImages() {
        val text = "![a](https://x.example/a.png)\n\n![b](https://x.example/b.png)"
        val hooks = MarkdownHooks(
            imageSize = { if (it.endsWith("a.png")) MarkdownSize(1600f, 900f) else null },
            loadedSize = { MarkdownSize(400f, 100f) },
        )
        val kinds = Markdown.prepare(text, hooks).map { it.kind as MarkdownBlockKind.Image }
        assertEquals(1600f, kinds[0].width)
        assertEquals(400f, kinds[1].width)
        assertEquals(100f, kinds[1].height)
        val none = Markdown.prepare(text, MarkdownHooks()).map { it.kind as MarkdownBlockKind.Image }
        assertEquals(null, none[1].width)
    }

    /** catalog/host.json `markdown.options.imageMaxHeight`: unset = the catalog's 160, null = none, invalid = 160; the measurer lays an uncapped image out at the width. */
    @Test
    fun imageMaxHeightOption() {
        Fixtures.require()
        assertEquals(MarkdownOptions.IMAGE_MAX_HEIGHT, MarkdownOptions().imageCap)
        assertEquals(null, MarkdownOptions(imageMaxHeight = null).imageCap)
        assertEquals(MarkdownOptions.IMAGE_MAX_HEIGHT, MarkdownOptions(imageMaxHeight = -3f).imageCap)
        assertEquals(240f, MarkdownOptions(imageMaxHeight = 240f).imageCap)
        val contract = JsonValue.parse(java.io.File(Fixtures.dir, "../catalog/host.json").readText())
        assertEquals(MarkdownOptions.IMAGE_MAX_HEIGHT.toDouble(), contract["markdown"]!!["options"]!!["imageMaxHeight"]!!["default"]!!.number!!, 0.0)
    }

    /** No hook (or no answer): exactly the blocks without them. */
    @Test
    fun noHooksPaintAsBefore() {
        val m = model(object : HostPlugin {})
        val text = "see #ABC-1\n\n[clip](https://app.example/a/clip.mp4)\n\n![w](https://app.example/a/w.png)"
        val plain = Markdown.policeLinks(Markdown.resolveImages(Markdown.parse(text)) { m.mediaRequest(it) != null }) { m.href(it) }
        assertEquals(plain, m.markdownBlocks(text))
        assertNull(m.host.onImagePress)
    }

    /** ExponentialHost: the policy's closures win, the base plugin's answer otherwise. */
    @Test
    fun exponentialHostBridgesTheHooks() {
        val pressed = ArrayList<String>()
        val h = ExponentialHost(
            HostOptions(
                policy = HostPolicy(
                    imageSize = { if (it == "a") ImageSize(10.0, 20.0) else null },
                    attachment = { if (it == "f") HostAttachment("file", "f.pdf") else null },
                    onImagePress = { pressed.add(it) },
                ),
            ),
            CoroutineScope(Dispatchers.Unconfined),
        )
        val bridged = h.surfaceHost
        assertEquals(ImageSize(10.0, 20.0), bridged.imageSize("a"))
        assertNull(bridged.imageSize("b"))
        assertEquals(HostAttachment("file", "f.pdf"), bridged.attachment("f"))
        bridged.onImagePress!!.invoke("x")
        assertEquals(listOf("x"), pressed)
    }

    /** The layout: an image the host sized reserves its box, attachments their natives' heights. */
    @Test
    fun theLayoutReservesHostSizes() {
        val s = MarkdownStyles(MarkdownTextSpec(14f, 20f, 400, null))
        val measure = object : MarkdownTextMeasure {
            override fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?): Float = spec.lineHeight
            override fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float = 8f * Markdown.plain(inlines).length
            override fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float = 8f
        }
        val f = fixture()
        val m = model(FakeMarkdownHost(f))
        val a = "https://app.example/api/attachments/"
        fun height(text: String, w: Float = 320f) = Markdown.layout(m.markdownBlocks(text), s, w, measure).height
        assertEquals("unknown = the fixed box", s.imageHeight, height("![x](${a}unknown.png)"))
        assertEquals("1600 × 800 in 200 = 100 tall", 100f, height("![x](${a}wide.png)", 200f))
        assertEquals("video: min(available, 320) / (16 / 9)", 320f * 9f / 16f, height("[c](${a}clip.mp4)", 400f), 0.01f)
        assertEquals("video in a narrow column", 200f * 9f / 16f, height("[c](${a}clip.mp4)", 200f), 0.01f)
        assertEquals("audio: title + gap + row", s.audioTitle + s.audioGap + s.audioRow, height("[v](${a}voice.m4a)"))
        assertEquals("file: the row", s.fileHeight, height("[r](${a}report.pdf)"))
    }

    /** The default chip geometry: 2 × padH + label (+ icon + gap). */
    @Test
    fun chipWidthAddsPaddingAndIcon() {
        val chips = MarkdownChipStyle()
        val chip = at.exponential.ui.primitives.MarkdownChip("issue", "ABC-1", "ui-issue")
        assertEquals(2 * 8f + 40f + 16f + 4f, chips.width(chip, 40f))
        assertEquals(2 * 8f + 40f, chips.width(chip.copy(icon = null), 40f))
        assertEquals("never taller than the line", 16f, chips.height(16f))
        assertEquals(20f, chips.height(24f))
    }
}
