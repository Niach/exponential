package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import at.exponential.ui.primitives.ImageSource
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1948: replays `packages/domain-contract/fixtures/image-actions.json`
// whole (TS `image-actions.test.ts`): `menuCases`, `pressCases`,
// `encodingCases`, `shareNameCases`, `sourceCases`, and locks the copy, the
// icons and the test ids.
class ImageActionsTest {
    private fun read(relative: String): JsonValue {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return JsonValue.parse(file.readText())
    }

    private val fixture = read("packages/domain-contract/fixtures/image-actions.json")

    private fun str(v: JsonValue?): String? = v?.takeIf { it != JsonValue.Null }?.string

    @Test
    fun `the copy, icons and test ids match the fixture`() {
        val copy = fixture["copy"]!!
        assertEquals(ImageActions.Copy.COPY_IMAGE, str(copy["copyImage"]))
        assertEquals(ImageActions.Copy.SHARE, str(copy["share"]))
        assertEquals(ImageActions.Copy.COPIED, str(copy["copied"]))
        assertEquals(ImageActions.Copy.COPY_FAILED, str(copy["copyFailed"]))
        assertEquals(ImageActions.Copy.SHARE_FAILED, str(copy["shareFailed"]))
        assertEquals(ImageActions.Copy.LINK_HOST, str(copy["linkHost"]))
        assertEquals(ImageActions.Icons.COPY_IMAGE, str(fixture["icons"]!!["copyImage"]))
        assertEquals(ImageActions.Icons.SHARE, str(fixture["icons"]!!["share"]))
        assertEquals(ImageActions.TestIds.COPY_IMAGE, str(fixture["testIds"]!!["copyImage"]))
        assertEquals(ImageActions.TestIds.SHARE, str(fixture["testIds"]!!["share"]))
        assertEquals(listOf("touch"), fixture["inputs"]!!["android"]!!.array!!.map { it.string })
    }

    @Test
    fun menuCases() {
        val cases = fixture["menuCases"]!!.array!!
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val input = if (c["input"]!!.string == "pointer") ImageActions.Input.Pointer else ImageActions.Input.Touch
            val canShare = c["canShareFile"]!!.bool!!
            val expected = c["expected"]!!.array!!.map { it.string }
            val actual = ImageActions.imageMenu(input, canShare).map { if (it == ImageActions.Item.CopyImage) "copyImage" else "share" }
            assertEquals("$input $canShare", expected, actual)
        }
    }

    @Test
    fun pressCases() {
        for (c in fixture["pressCases"]!!.array!!) {
            val actual = ImageActions.pressMenu(c["onImage"]!!.bool!!, c["imageReady"]!!.bool!!, str(c["ownerMenu"]))
            assertEquals(c.json, c["expected"]!!.string, actual.name.lowercase())
        }
    }

    @Test
    fun encodingCases() {
        for (c in fixture["encodingCases"]!!.array!!) {
            val head = c["head"]!!.string!!.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
            assertEquals(c.json, c["expected"]!!.string, ImageActions.copyEncoding(head).name.lowercase())
        }
    }

    @Test
    fun shareNameCases() {
        for (c in fixture["shareNameCases"]!!.array!!) {
            assertEquals(c.json, c["expected"]!!.string, ImageActions.shareFileName(str(c["name"])))
        }
    }

    @Test
    fun sourceCases() {
        val origin = fixture["sourcePolicy"]!!["origin"]!!.string!!
        val cases = fixture["sourceCases"]!!.array!!
        assertTrue(cases.size > 40)
        for (c in cases) {
            val e = c["expected"]!!
            val expected = when (e["kind"]!!.string) {
                "load" -> ImageSource.Load
                "link" -> ImageSource.Link(e["href"]!!.string!!, e["text"]!!.string!!)
                else -> ImageSource.Text(e["text"]!!.string!!)
            }
            assertEquals(c.json, expected, ImageActions.imageSource(c["src"]!!.string!!, c["alt"]!!.string!!, origin))
        }
    }

    @Test
    fun `an attachment id comes from our attachment urls only`() {
        val id = "0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90"
        assertEquals(id, ImageActions.attachmentId("/api/attachments/$id?w=480", "https://app.exponential.at"))
        assertEquals(id, ImageActions.attachmentId("https://app.exponential.at/api/attachments/$id", "https://app.exponential.at"))
        assertEquals(null, ImageActions.attachmentId("https://example.com/api/attachments/$id", "https://app.exponential.at"))
    }
}
