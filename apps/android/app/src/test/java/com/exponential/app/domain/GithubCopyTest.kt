package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// SLOP-7/SLOP-26: the connection block + picker copy is byte-identical ×4.
// These literals mirror web `github-connect-copy.ts`
// (`github-connect-copy.test.ts`) — change them together.
class GithubCopyTest {
    @Test
    fun connectionBlockLiterals() {
        assertEquals("Repositories", GithubCopy.SECTION_TITLE)
        assertEquals("Add repository", GithubCopy.ADD_REPOSITORY)
        assertEquals(
            "Connect your GitHub account, install the Exponential app on the accounts whose repositories you want, then add repositories you can push to. Point a board at one to make it the clone target for “Start coding”.",
            GithubCopy.INTRO,
        )
        assertEquals("Couldn’t reach GitHub connect state.", GithubCopy.STATUS_FAILED)
        assertEquals("GitHub isn’t configured on this server.", GithubCopy.NOT_CONFIGURED)
        assertEquals("No GitHub account connected", GithubCopy.NOT_LINKED)
        assertEquals("Connect GitHub", GithubCopy.CONNECT_GITHUB)
        assertEquals("Connected as octocat", GithubCopy.connectedAs("octocat"))
        assertEquals("GitHub connected", GithubCopy.CONNECTED)
        assertEquals("Your GitHub connection expired.", GithubCopy.RECONNECT_NEEDED)
        assertEquals("Reconnect", GithubCopy.RECONNECT)
        assertEquals("Reconnect GitHub", GithubCopy.RECONNECT_GITHUB)
        assertEquals("Disconnect GitHub", GithubCopy.DISCONNECT_TITLE)
        assertEquals(
            "This unlinks GitHub from your account. Repositories already added keep working; adding more needs a reconnect.",
            GithubCopy.DISCONNECT_BODY,
        )
        assertEquals(
            "The Exponential app isn’t installed on any of your GitHub accounts yet.",
            GithubCopy.NOT_INSTALLED,
        )
        assertEquals("Install the app", GithubCopy.INSTALL_APP)
        assertEquals("Install on another account", GithubCopy.INSTALL_ANOTHER)
        assertEquals("GitHub accounts with the app installed", GithubCopy.ACCOUNTS_HEADER)
        assertEquals("Configure which repositories acme grants on GitHub", GithubCopy.configureTitle("acme"))
        assertEquals(
            "Repositories come from these accounts. Configure one to grant more.",
            GithubCopy.INSTALLATION_CAPTION,
        )
        assertEquals(
            "GitHub suspended the Exponential app for acme, octocat. Unsuspend it on GitHub.",
            GithubCopy.suspendedLine(listOf("acme", "octocat")),
        )
        assertEquals("No repositories added yet.", GithubCopy.NO_REPOSITORIES)
        assertEquals("GitHub didn’t finish connecting. Try again.", GithubCopy.LINK_FAILED)
    }

    @Test
    fun installationLabelFallsBackLowerCase() {
        assertEquals("acme", GithubCopy.installationLabel("acme", 7))
        assertEquals("installation 7", GithubCopy.installationLabel(null, 7))
        assertEquals("installation 7", GithubCopy.installationLabel("", 7))
    }

    @Test
    fun pickerLiterals() {
        assertEquals("Loading your GitHub repositories…", GithubCopy.PICKER_LOADING)
        assertEquals(
            "GitHub isn’t configured on this server, so repositories can’t be added.",
            GithubCopy.PICKER_NOT_CONFIGURED,
        )
        assertEquals(
            "Connect your GitHub account to pick a repository. You’ll come right back here.",
            GithubCopy.PICKER_NOT_LINKED,
        )
        assertEquals(
            "Install the Exponential app on the GitHub account that owns the repository. You’ll come right back here.",
            GithubCopy.PICKER_NOT_INSTALLED,
        )
        assertEquals("I’ve done that", GithubCopy.PICKER_CONNECTED_CHECK)
        assertEquals(
            "Your GitHub connection expired. Reconnect to list your repositories.",
            GithubCopy.PICKER_RECONNECT_BANNER,
        )
        assertEquals(
            "The app is installed, but none of its repositories lets you push. Grant one on GitHub, then refresh.",
            GithubCopy.NONE_PUSHABLE,
        )
        assertEquals(
            "Only repositories you can push to, on accounts where the app is installed, appear here. Missing one? Grant it on GitHub, then refresh.",
            GithubCopy.FOOTER,
        )
        assertEquals(
            "Showing the first 500 repositories per account — use the field below for the rest.",
            GithubCopy.CAP_NOTE,
        )
        assertEquals("Add repository by name", GithubCopy.LOOKUP_A11Y)
        assertEquals(
            "GitHub says you can’t push to this repository, or your connection expired. Reconnect GitHub and try again.",
            GithubCopy.ADD_FORBIDDEN,
        )
    }

    @Test
    fun suspendedPickerFallback() {
        assertEquals(
            "GitHub suspended the Exponential app for a connected account. Its repositories can’t be added until you unsuspend it on GitHub.",
            GithubCopy.pickerSuspended(listOf(null)),
        )
        assertEquals(
            "GitHub suspended the Exponential app for acme, a connected account. Its repositories can’t be added until you unsuspend it on GitHub.",
            GithubCopy.pickerSuspended(listOf("acme", "")),
        )
    }

    @Test
    fun grantForbiddenNeedsTheReconnectMessage() {
        assertTrue(GithubCopy.isGrantForbidden("FORBIDDEN", 403, "You don't have access… Reconnect GitHub in team settings"))
        assertFalse(GithubCopy.isGrantForbidden("FORBIDDEN", 403, "Only team members can do that"))
        assertFalse(GithubCopy.isGrantForbidden("PRECONDITION_FAILED", 412, "Reconnect GitHub"))
    }
}
