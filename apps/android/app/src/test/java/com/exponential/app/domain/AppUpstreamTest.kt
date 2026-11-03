package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// VAPP-230: replays `packages/emcp/fixtures/upstream.json` `binding.bannerCases`
// (web, desktop and iOS replay the same cases) and locks the constants.
class AppUpstreamTest {
    private fun read(relative: String): JsonValue {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return JsonValue.parse(file.readText())
    }

    private val fixture = read("packages/emcp/fixtures/upstream.json")
    private val binding = fixture["binding"]!!

    @Test
    fun `the constants match the fixture`() {
        assertEquals(binding["scheme"]!!.string, AppUpstream.SCHEME)
        assertEquals(binding["cacheSeconds"]!!.number!!.toInt(), AppUpstream.CACHE_SECONDS)
        assertEquals(binding["banner"]!!.string, AppUpstream.BANNER)
        assertEquals(fixture["errors"]!!["disconnected"]!!.string, AppUpstream.DISCONNECTED)
    }

    @Test
    fun `the banner cases replay`() {
        val cases = binding["bannerCases"]!!.array!!
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val answers = case["answers"]!!.array!!.map(AppUpstream.Answer::of)
            val expected = case["expected"]!!.array!!.map { it.string!! }
            assertEquals(case["name"]!!.string, expected, AppUpstream.upstreamBanners(answers))
        }
    }

    @Test
    fun `an ok answer carries its value`() {
        val answer = AppUpstream.Answer.of(JsonValue.parse("""{"ok":true,"value":{"mrr":1200},"meta":{}}"""))
        assertEquals(true, answer.ok)
        assertEquals(1200.0, answer.value!!["mrr"]!!.number)
    }
}
