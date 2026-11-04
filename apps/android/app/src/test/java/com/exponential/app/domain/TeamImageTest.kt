package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import com.exponential.app.data.api.updateTeamIconInput
import java.io.File
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1645: replays `packages/domain-contract/fixtures/team-image.json`'s
// `markCases` whole (TS `team-image.test.ts`, desktop `domain::team_image`, iOS
// `TeamImageTests`) and locks the copy, test ids, mark, row and upload numbers.
class TeamImageTest {
    private fun read(relative: String): JsonValue {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return JsonValue.parse(file.readText())
    }

    private val fixture = read("packages/domain-contract/fixtures/team-image.json")

    @Test
    fun `the copy and the test ids match the fixture`() {
        assertEquals(TeamImage.Copy.ROW, fixture["copy"]!!["row"]!!.string)
        assertEquals(TeamImage.Copy.UPLOAD, fixture["copy"]!!["upload"]!!.string)
        assertEquals(TeamImage.Copy.REMOVE, fixture["copy"]!!["remove"]!!.string)
        assertEquals(TeamImage.TestIds.ROW, fixture["testIds"]!!["row"]!!.string)
        assertEquals(TeamImage.TestIds.UPLOAD, fixture["testIds"]!!["upload"]!!.string)
        assertEquals(TeamImage.TestIds.REMOVE, fixture["testIds"]!!["remove"]!!.string)
    }

    @Test
    fun `the mark, row and upload numbers match the fixture`() {
        assertEquals(fixture["mark"]!!["radiusFraction"]!!.number!!.toFloat(), TeamImage.RADIUS_FRACTION)
        assertEquals(TeamImage.GLYPH, fixture["mark"]!!["glyph"]!!.string)
        assertEquals(TeamImage.ROW_MARK_SIZE, fixture["row"]!!["markSize"]!!.number!!.toInt())
        val upload = fixture["upload"]!!
        assertEquals(TeamImage.Upload.PATH, upload["path"]!!.string)
        assertEquals(TeamImage.Upload.MAX_BYTES, upload["maxBytes"]!!.number!!.toLong())
        assertEquals(TeamImage.Upload.SIDE, upload["side"]!!.number!!.toInt())
        assertEquals(TeamImage.Upload.ACCEPT, upload["accept"]!!.array!!.map { it.string!! })
    }

    @Test
    fun `markCases`() {
        val cases = fixture["markCases"]!!.array!!
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val iconUrl = case["iconUrl"]?.takeIf { it != JsonValue.Null }?.string
            val mark = case["mark"]!!
            val expected = when (mark["kind"]!!.string) {
                "image" -> TeamImage.Mark.Image(mark["src"]!!.string!!)
                "glyph" -> TeamImage.Mark.Glyph
                else -> error("unknown mark ${mark["kind"]}")
            }
            assertEquals("$iconUrl", expected, TeamImage.teamMark(iconUrl))
        }
    }

    @Test
    fun `the upload path names the team`() {
        assertEquals("/api/teams/t1/icon", TeamImage.uploadPath("t1"))
    }

    @Test
    fun `removing travels as a literal null`() {
        // `explicitNulls = false` would drop it off a @Serializable class, and
        // the router applies a field only when its key is present.
        val input = updateTeamIconInput("t1")
        assertEquals("t1", input["teamId"]!!.jsonPrimitive.content)
        assertEquals(JsonNull, input["iconUrl"])
    }
}
