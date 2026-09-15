package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// FEED-42: the connection block + picker copy is byte-identical ×4. These
// literals mirror web `github-connect-copy.ts` — change them together.
class GithubCopyTest {
    @Test
    fun connectionBlockLiterals() {
        assertEquals("Couldn’t reach GitHub connect state.", GithubCopy.STATUS_FAILED)
        assertEquals("GitHub isn’t configured on this server.", GithubCopy.NOT_CONFIGURED)
        assertEquals("No GitHub account connected", GithubCopy.NOT_INSTALLED)
        assertEquals("GitHub accounts connected to this team", GithubCopy.ACCOUNTS_HEADER)
        assertEquals("No repositories connected yet.", GithubCopy.NO_REPOSITORIES)
        assertEquals(
            "Connect a GitHub account or organization first, then add its repositories to share them with the team — everyone can code on a shared repo. Point a board at one to make it the clone target for “Start coding”.",
            GithubCopy.INTRO,
        )
        assertEquals(
            "An installation is per GitHub account or organization. Repositories come from the accounts listed here.",
            GithubCopy.INSTALLATION_CAPTION,
        )
    }

    @Test
    fun installationLabelFallsBackLowerCase() {
        assertEquals("acme", GithubCopy.installationLabel("acme", 7))
        assertEquals("installation 7", GithubCopy.installationLabel(null, 7))
        assertEquals("installation 7", GithubCopy.installationLabel("", 7))
    }

    @Test
    fun confirmCopy() {
        assertEquals(
            "This disconnects acme from the team. Repositories connected through it must be removed first.",
            GithubCopy.unlinkConfirm("acme"),
        )
        assertEquals(
            "This removes acme from the team. Nobody’s GitHub connection covers it, so no repositories are lost.",
            GithubCopy.staleConfirm("acme"),
        )
        assertEquals(
            "No one’s GitHub connection covers acme anymore — reconnecting can’t refresh it.",
            GithubCopy.staleLine("acme"),
        )
    }

    @Test
    fun pickerLiterals() {
        assertEquals("Loading your GitHub repositories…", GithubCopy.PICKER_LOADING)
        assertEquals("I’ve connected", GithubCopy.I_HAVE_CONNECTED)
        assertEquals(
            "Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh.",
            GithubCopy.FOOTER,
        )
        assertEquals("Add repository by name", GithubCopy.LOOKUP_A11Y)
        assertEquals(
            "GitHub says you don’t have access to this repository, or your connection is stale. Reconnect GitHub and try again.",
            GithubCopy.ADD_FORBIDDEN,
        )
    }

    @Test
    fun reauthBannerVariants() {
        assertEquals(
            "Reconnect GitHub (a, b) to refresh. Repos created or shared with you since your last connect won’t appear until you do.",
            GithubCopy.reauthBanner(empty = false, logins = listOf("a", "b")),
        )
        assertEquals(
            "Reconnect GitHub to load the repositories you can access from a.",
            GithubCopy.reauthBanner(empty = true, logins = listOf("a")),
        )
        assertEquals(
            "Reconnect GitHub to load the repositories you can access.",
            GithubCopy.reauthBanner(empty = true, logins = emptyList()),
        )
    }

    @Test
    fun suspendedPickerFallback() {
        assertEquals(
            "GitHub suspended the Exponential app for acme, a connected account. Its repositories can’t be connected until you unsuspend it on GitHub.",
            GithubCopy.pickerSuspended(listOf("acme", null)),
        )
    }

    @Test
    fun grantForbiddenNeedsTheReconnectMessage() {
        assertTrue(GithubCopy.isGrantForbidden("FORBIDDEN", 403, "You don't have access… Reconnect GitHub in team settings"))
        assertFalse(GithubCopy.isGrantForbidden("FORBIDDEN", 403, "Only team members can do that"))
        assertFalse(GithubCopy.isGrantForbidden("PRECONDITION_FAILED", 412, "Reconnect GitHub"))
    }
}
