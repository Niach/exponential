package com.exponential.app.data.auth

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Unit tests for the pure per-user account state machine. The invariant under
 * test: a login keys the account by (server, userId), so different users of the
 * same server never share an account id (hence never share a Room DB file).
 */
class AccountResolverTest {

    private val url = "https://team.example"

    private fun resolve(
        accounts: List<ServerAccount>,
        userId: String,
        token: String,
    ) = resolveAccounts(
        accounts = accounts,
        instanceUrl = url,
        userId = userId,
        token = token,
        email = null,
        name = null,
        isAdmin = false,
        onboardingCompletedAt = null,
        onboardingKnown = false,
        now = 1_000L,
    )

    @Test
    fun idOverloadsAreDistinctPerUser() {
        assertNotEquals(ServerAccount.makeId(url), ServerAccount.makeId(url, "A"))
        assertNotEquals(ServerAccount.makeId(url, "A"), ServerAccount.makeId(url, "B"))
    }

    @Test
    fun rekeysPendingAccountToPerUserId() {
        val pending = ServerAccount(id = ServerAccount.makeId(url), instanceUrl = url, token = "t")
        val result = resolve(listOf(pending), userId = "A", token = "tokA")

        val resolvedId = ServerAccount.makeId(url, "A")
        assertEquals(resolvedId, result.activeId)
        assertEquals(1, result.accounts.size)
        val account = result.accounts.single()
        assertEquals(resolvedId, account.id)
        assertEquals("tokA", account.token)
        assertEquals("A", account.userId)
        // The pending URL-only record is consumed.
        assertNull(result.accounts.firstOrNull { it.id == ServerAccount.makeId(url) })
    }

    @Test
    fun differentUserGetsDistinctAccountLeavingFirstUntouched() {
        val userA = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = "tokA", userId = "A",
        )
        val pending = ServerAccount(id = ServerAccount.makeId(url), instanceUrl = url, token = "tokB")
        val result = resolve(listOf(userA, pending), userId = "B", token = "tokB")

        val idB = ServerAccount.makeId(url, "B")
        assertEquals(idB, result.activeId)
        assertEquals(2, result.accounts.size)
        // A's account (and its DB, keyed by A's id) is untouched.
        val a = result.accounts.firstOrNull { it.id == ServerAccount.makeId(url, "A") }
        assertNotNull(a)
        assertEquals("tokA", a!!.token)
        // B's account is created fresh.
        val b = result.accounts.firstOrNull { it.id == idB }
        assertNotNull(b)
        assertEquals("tokB", b!!.token)
        assertNull(result.accounts.firstOrNull { it.id == ServerAccount.makeId(url) })
    }

    @Test
    fun sameUserReloginMergesIntoExistingAccountWithoutDuplicating() {
        val existing = ServerAccount(
            id = ServerAccount.makeId(url, "A"),
            instanceUrl = url,
            token = "old",
            userId = "A",
            onboardingCompletedAt = "2026-01-01",
            onboardingKnown = true,
        )
        val pending = ServerAccount(id = ServerAccount.makeId(url), instanceUrl = url, token = "new")
        val result = resolve(listOf(existing, pending), userId = "A", token = "new")

        assertEquals(ServerAccount.makeId(url, "A"), result.activeId)
        assertEquals(1, result.accounts.size)
        assertEquals("new", result.accounts.single().token)
    }

    @Test
    fun tokenRefreshOnAlreadyResolvedAccountUpdatesInPlace() {
        val existing = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = "old", userId = "A",
        )
        val result = resolve(listOf(existing), userId = "A", token = "new")

        assertEquals(ServerAccount.makeId(url, "A"), result.activeId)
        assertEquals(1, result.accounts.size)
        assertEquals("new", result.accounts.single().token)
    }

    @Test
    fun wipesLegacyDbForRekeyedAccount() {
        val rekeyed = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = "t", userId = "A",
        )
        assertEquals(ServerAccount.makeId(url), legacyDbIdToWipe(rekeyed))
    }

    @Test
    fun wipesLegacyDbForTokenlessUrlKeyedAccount() {
        // The migration nulled a userId-less account's token but left it keyed by
        // the URL-only id — its DB is the wrong-user cache the cleanup targets.
        val tokenless = ServerAccount(id = ServerAccount.makeId(url), instanceUrl = url, token = null)
        assertEquals(ServerAccount.makeId(url), legacyDbIdToWipe(tokenless))
    }

    @Test
    fun keepsLegacyDbForLiveUrlKeyedAccount() {
        // Defensive: a URL-keyed account that still has a token isn't a stale
        // cache to reap (shouldn't reach cleanup post-migration, but never wipe
        // a live account's DB out from under it).
        val live = ServerAccount(id = ServerAccount.makeId(url), instanceUrl = url, token = "t")
        assertNull(legacyDbIdToWipe(live))
    }

    @Test
    fun removesSignedOutDuplicateWhenSignedInSiblingExists() {
        val signedOut = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = null, userId = "A",
        )
        val signedIn = ServerAccount(
            id = ServerAccount.makeId(url, "B"), instanceUrl = url, token = "tokB", userId = "B",
        )
        val ids = duplicateSignedOutAccountIds(
            listOf(signedOut, signedIn),
            activeId = signedIn.id,
        )
        assertEquals(listOf(signedOut.id), ids)
    }

    @Test
    fun keepsLoneSignedOutAccount() {
        // The re-login affordance: nothing else on this server is signed in.
        val signedOut = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = null, userId = "A",
        )
        assertEquals(
            emptyList<String>(),
            duplicateSignedOutAccountIds(listOf(signedOut), activeId = signedOut.id),
        )
    }

    @Test
    fun neverRemovesSignedInAccounts() {
        // Two users signed in on the same server is legal multi-account state.
        val a = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = "tokA", userId = "A",
        )
        val b = ServerAccount(
            id = ServerAccount.makeId(url, "B"), instanceUrl = url, token = "tokB", userId = "B",
        )
        assertEquals(
            emptyList<String>(),
            duplicateSignedOutAccountIds(listOf(a, b), activeId = b.id),
        )
    }

    @Test
    fun neverRemovesTheActiveAccount() {
        // The active row is signed out (its session died) while a background
        // account on the same server still is: it stays, or the login screen
        // would lose the server it is sitting on.
        val active = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = null, userId = "A",
        )
        val other = ServerAccount(
            id = ServerAccount.makeId(url, "B"), instanceUrl = url, token = "tokB", userId = "B",
        )
        assertEquals(
            emptyList<String>(),
            duplicateSignedOutAccountIds(listOf(active, other), activeId = active.id),
        )
    }

    @Test
    fun neverDedupesAcrossDifferentInstances() {
        val signedOutHere = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = null, userId = "A",
        )
        val signedInElsewhere = ServerAccount(
            id = ServerAccount.makeId("https://other.example", "B"),
            instanceUrl = "https://other.example",
            token = "otherTok",
            userId = "B",
        )
        assertEquals(
            emptyList<String>(),
            duplicateSignedOutAccountIds(
                listOf(signedOutHere, signedInElsewhere),
                activeId = signedInElsewhere.id,
            ),
        )
    }

    @Test
    fun postLoginDedupeKeepsTheNewRowAndDropsTheStaleOne() {
        // The bug: the server-side account was deleted and signed up again, so
        // the same email comes back as a NEW userId on the same instance.
        val dead = ServerAccount(
            id = ServerAccount.makeId(url, "A"), instanceUrl = url, token = null, userId = "A",
        )
        val pending = ServerAccount(id = ServerAccount.makeId(url), instanceUrl = url)
        val resolved = resolve(listOf(dead, pending), userId = "B", token = "tokB")
        assertEquals(2, resolved.accounts.size)

        val ids = duplicateSignedOutAccountIds(resolved.accounts, resolved.activeId)
        assertEquals(listOf(dead.id), ids)
        // What survives is exactly the row this login wrote.
        val kept = resolved.accounts.filterNot { it.id in ids }
        assertEquals(listOf(ServerAccount.makeId(url, "B")), kept.map { it.id })
        assertEquals("tokB", kept.single().token)
    }

    @Test
    fun leavesAccountsForOtherServersUntouched() {
        val other = ServerAccount(
            id = ServerAccount.makeId("https://other.example", "A"),
            instanceUrl = "https://other.example",
            token = "otherTok",
            userId = "A",
        )
        val pending = ServerAccount(id = ServerAccount.makeId(url), instanceUrl = url, token = "t")
        val result = resolve(listOf(other, pending), userId = "A", token = "tokA")

        assertNotNull(result.accounts.firstOrNull { it.id == other.id && it.token == "otherTok" })
        assertNotNull(result.accounts.firstOrNull { it.id == ServerAccount.makeId(url, "A") })
    }
}
