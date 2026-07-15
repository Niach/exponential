package com.exponential.app.domain

import java.net.URI
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-92: the App-Link parser must stay the exact inverse of [WebLinks.issueUrl]
 * — these are the only two path shapes the manifest's autoVerify filter claims.
 */
class WebLinksTest {
    @Test
    fun parsesIssueUrl() {
        assertEquals(
            WebLinks.Parsed.IssueRef("acme", "web", "EXP-42"),
            WebLinks.parsePath("/w/acme/projects/web/issues/EXP-42"),
        )
    }

    // The web moved /w/ → /t/ (workspaces → teams rename, EXP-122). Both prefixes
    // must resolve identically — /t/ is the new mint, /w/ stays valid forever.
    @Test
    fun parsesTeamPrefixedIssueUrl() {
        assertEquals(
            WebLinks.Parsed.IssueRef("acme", "web", "EXP-42"),
            WebLinks.parsePath("/t/acme/projects/web/issues/EXP-42"),
        )
    }

    @Test
    fun toleratesTrailingSlashOnTeamPrefix() {
        assertEquals(
            WebLinks.Parsed.IssueRef("acme", "web", "EXP-42"),
            WebLinks.parsePath("/t/acme/projects/web/issues/EXP-42/"),
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
            workspaceSlug = "acme",
            projectSlug = "web",
            identifier = "EXP-42",
        )
        assertEquals(
            WebLinks.Parsed.IssueRef("acme", "web", "EXP-42"),
            WebLinks.parsePath(URI(minted).path),
        )
    }

    @Test
    fun toleratesTrailingSlash() {
        assertEquals(
            WebLinks.Parsed.IssueRef("acme", "web", "EXP-42"),
            WebLinks.parsePath("/w/acme/projects/web/issues/EXP-42/"),
        )
    }

    @Test
    fun rejectsUnclaimedPaths() {
        for (path in listOf(
            null, "", "/", "/w/acme", "/w/acme/projects/web", "/w/acme/inbox",
            "/w/acme/projects/web/issues", "/w/acme/projects/web/issues/EXP-1/changes",
            // The /t/ twin is claimed only at the exact issue depth, same as /w/.
            "/t/acme", "/t/acme/projects/web", "/t/acme/inbox",
            "/t/acme/projects/web/issues", "/t/acme/projects/web/issues/EXP-1/changes",
            "/invite", "/auth/login",
        )) {
            assertNull("expected null for $path", WebLinks.parsePath(path))
        }
    }
}
