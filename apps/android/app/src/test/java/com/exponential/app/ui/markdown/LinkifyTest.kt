package com.exponential.app.ui.markdown

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-430: narration URLs become tappable links — the remote `/login` flow
// publishes the claude sign-in URL as narration, and it must tokenize
// byte-for-byte intact. Mirrors web `lib/linkify.test.ts`. The ranges are what
// the markdown renderer's autolink pass places its link spans by (EXP-440), so
// every case asserts the offsets, not just the extracted text.
class LinkifyTest {

    private val signInUrl =
        "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e" +
            "&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback" +
            "&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainference&code_challenge=j7BY1qKMJ1Y2LC5xNqD5" +
            "VUJayK_UZbPl_FCJLsmPZzk&code_challenge_method=S256&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE"

    /** Every range must name exactly the characters it spans. */
    private fun assertRangesMatchSource(text: String, ranges: List<UrlRange>) {
        for (range in ranges) {
            assertTrue("range within text", range.start >= 0 && range.end <= text.length)
            assertEquals(range.url, text.substring(range.start, range.end))
        }
    }

    @Test
    fun plainTextHasNoUrls() {
        assertTrue(urlRanges("Session started").isEmpty())
        assertTrue(urlRanges("").isEmpty())
    }

    @Test
    fun signInUrlSurvivesIntact() {
        val text = "Claude sign-in: open this link in your browser:\n\n$signInUrl"
        val ranges = urlRanges(text)
        assertEquals(1, ranges.size)
        assertEquals(signInUrl, ranges[0].url)
        assertRangesMatchSource(text, ranges)
    }

    @Test
    fun urlInProseCarriesItsSourceOffsets() {
        val text = "Opened https://github.com/x/y/pull/12 for review"
        val ranges = urlRanges(text)
        assertEquals(1, ranges.size)
        assertEquals(7, ranges[0].start)
        assertEquals("https://github.com/x/y/pull/12", ranges[0].url)
        assertRangesMatchSource(text, ranges)
    }

    @Test
    fun trailingProsePunctuationStaysOutOfTheRange() {
        val text = "(see https://x.dev/docs)."
        val ranges = urlRanges(text)
        assertEquals(1, ranges.size)
        assertEquals("https://x.dev/docs", ranges[0].url)
        assertRangesMatchSource(text, ranges)
    }

    @Test
    fun balancedParensStayInTheUrl() {
        val url = "https://en.wikipedia.org/wiki/Bracket_(disambiguation)"
        val ranges = urlRanges(url)
        assertEquals(listOf(url), ranges.map { it.url })
        assertRangesMatchSource(url, ranges)
    }

    @Test
    fun multipleUrlsAllTokenize() {
        val text = "a https://one.test b http://two.test c"
        val ranges = urlRanges(text)
        assertEquals(listOf("https://one.test", "http://two.test"), ranges.map { it.url })
        assertRangesMatchSource(text, ranges)
    }
}
