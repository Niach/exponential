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
