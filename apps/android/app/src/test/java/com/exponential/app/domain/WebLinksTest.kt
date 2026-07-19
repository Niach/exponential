package com.exponential.app.domain

import java.net.URI
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-92: the App-Link parser must stay the exact inverse of [WebLinks.issueUrl]
 * — these are the only two path shapes the manifest's autoVerify filter claims.
 * Since the great rename (EXP-180) that is exclusively the
 * `/t/{team}/boards/{board}/issues/{id}` form: `/w/` and `/projects/` are dead
 * on the web (no redirects), so the app must not claim them either.
 */
class WebLinksTest {
    @Test
    fun parsesIssueUrl() {
        assertEquals(
            WebLinks.Parsed.IssueRef("acme", "web", "EXP-42"),
            WebLinks.parsePath("/t/acme/boards/web/issues/EXP-42"),
        )
    }

    @Test
    fun toleratesTrailingSlash() {
        assertEquals(
            WebLinks.Parsed.IssueRef("acme", "web", "EXP-42"),
            WebLinks.parsePath("/t/acme/boards/web/issues/EXP-42/"),
        )
    }

    @Test
    fun parsesInviteUrl() {
        assertEquals(WebLinks.Parsed.Invite("abc123"), WebLinks.parsePath("/invite/abc123"))
    }

    @Test
    fun mintParseRoundTrip() {
        val minted = WebLinks.issueUrl(
            base = "https://app.exponential.at/",
            teamSlug = "acme",
            boardSlug = "web",
            identifier = "EXP-42",
        )
        assertEquals(
            "https://app.exponential.at/t/acme/boards/web/issues/EXP-42",
            minted,
        )
        assertEquals(
            WebLinks.Parsed.IssueRef("acme", "web", "EXP-42"),
            WebLinks.parsePath(URI(minted).path),
        )
    }

    // EXP-188: the onboarding/join flows accept a pasted invite LINK or a bare
    // token — mirror of the desktop `extract_token` helper.
    @Test
    fun extractsInviteTokenFromLinksAndRawTokens() {
        assertEquals("tok123", WebLinks.extractInviteToken("https://app.exponential.at/invite/tok123"))
        assertEquals("tok123", WebLinks.extractInviteToken("https://app.exponential.at/invite/tok123?x=1"))
        assertEquals("tok123", WebLinks.extractInviteToken("https://app.exponential.at/invite/tok123#frag"))
        assertEquals("tok123", WebLinks.extractInviteToken("https://app.exponential.at/invite/tok123/"))
        assertEquals("tok123", WebLinks.extractInviteToken("  tok123  "))
    }

    @Test
    fun rejectsUnextractableInviteInput() {
        assertNull(WebLinks.extractInviteToken(""))
        assertNull(WebLinks.extractInviteToken("   "))
        assertNull(WebLinks.extractInviteToken("https://app.exponential.at/invite/"))
        assertNull(WebLinks.extractInviteToken("not a token"))
    }

    @Test
    fun rejectsUnclaimedPaths() {
        for (path in listOf(
            null, "", "/", "/t/acme", "/t/acme/boards/web", "/t/acme/inbox",
            "/t/acme/boards/web/issues", "/t/acme/boards/web/issues/EXP-1/changes",
            // Legacy pre-rename forms are DEAD (EXP-180) — never claimed.
            "/w/acme/boards/web/issues/EXP-1",
            "/w/acme/projects/web/issues/EXP-1",
            "/t/acme/projects/web/issues/EXP-1",
            "/invite", "/auth/login",
        )) {
            assertNull("expected null for $path", WebLinks.parsePath(path))
        }
    }
}
