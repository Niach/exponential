package com.exponential.app.ui.session

import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-430: narration URLs become tappable links — the remote `/login` flow
// publishes the claude sign-in URL as narration, and it must tokenize
// byte-for-byte intact. Mirrors web `lib/linkify.test.ts`.
class LinkifyTest {

    private val signInUrl =
        "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e" +
            "&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback" +
            "&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainference&code_challenge=j7BY1qKMJ1Y2LC5xNqD5" +
            "VUJayK_UZbPl_FCJLsmPZzk&code_challenge_method=S256&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE"

    @Test
    fun plainTextIsOneSegment() {
        assertEquals(listOf(LinkSegment("Session started")), linkSegments("Session started"))
        assertEquals(listOf(LinkSegment("")), linkSegments(""))
    }

    @Test
    fun signInUrlSurvivesIntact() {
        val text = "Claude sign-in: open this link in your browser:\n\n$signInUrl"
        val segments = linkSegments(text)
        assertEquals(2, segments.size)
        assertEquals(LinkSegment(signInUrl, href = signInUrl), segments[1])
        assertEquals(text, segments.joinToString("") { it.text })
    }

    @Test
    fun urlInProseKeepsSurroundingText() {
        assertEquals(
            listOf(
                LinkSegment("Opened "),
                LinkSegment("https://github.com/x/y/pull/12", href = "https://github.com/x/y/pull/12"),
                LinkSegment(" for review"),
            ),
            linkSegments("Opened https://github.com/x/y/pull/12 for review"),
        )
    }

    @Test
    fun trailingProsePunctuationStaysOutOfTheLink() {
        assertEquals(
            listOf(
                LinkSegment("(see "),
                LinkSegment("https://x.dev/docs", href = "https://x.dev/docs"),
                LinkSegment(")."),
            ),
            linkSegments("(see https://x.dev/docs)."),
        )
    }

    @Test
    fun balancedParensStayInTheLink() {
        val url = "https://en.wikipedia.org/wiki/Bracket_(disambiguation)"
        assertEquals(listOf(LinkSegment(url, href = url)), linkSegments(url))
    }

    @Test
    fun multipleUrlsAllLink() {
        val text = "a https://one.test b http://two.test c"
        val segments = linkSegments(text)
        assertEquals(2, segments.count { it.href != null })
        assertEquals(text, segments.joinToString("") { it.text })
    }
}
