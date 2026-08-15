package com.exponential.app.data.images

import com.exponential.app.data.auth.ServerAccount
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Unit tests for [resolveAccountForUrl] — the Coil interceptor's token pick.
 * The invariant under test (REV-3): with several signed-in accounts on the
 * SAME server (the store keeps them oldest-first), the ACTIVE account's token
 * must win, not the oldest match's.
 */
class InstanceUrlAccountSelectionTest {

    private val url = "https://team.example"

    private fun account(
        userId: String,
        token: String? = "tok-$userId",
        instanceUrl: String = url,
        lastUsedAt: Long = 0L,
    ) = ServerAccount(
        id = ServerAccount.makeId(instanceUrl, userId),
        instanceUrl = instanceUrl,
        token = token,
        userId = userId,
        lastUsedAt = lastUsedAt,
    )

    @Test
    fun activeAccountWinsOverOlderSameServerAccount() {
        val older = account("A", lastUsedAt = 2_000L)
        val active = account("B", lastUsedAt = 1_000L)
        val picked = resolveAccountForUrl(
            accounts = listOf(older, active),
            activeAccountId = active.id,
            absoluteUrl = "$url/api/attachments/xyz",
        )
        assertEquals(active.id, picked?.id)
    }

    @Test
    fun fallsBackToMostRecentlyUsedWhenActiveIsElsewhere() {
        val older = account("A", lastUsedAt = 1_000L)
        val recent = account("B", lastUsedAt = 2_000L)
        val elsewhere = account("C", instanceUrl = "https://other.example")
        val picked = resolveAccountForUrl(
            accounts = listOf(older, recent, elsewhere),
            activeAccountId = elsewhere.id,
            absoluteUrl = "$url/api/attachments/xyz",
        )
        assertEquals(recent.id, picked?.id)
    }

    @Test
    fun signedOutAccountNeverWinsAMatch() {
        val signedOut = account("A", token = null, lastUsedAt = 2_000L)
        val signedIn = account("B", lastUsedAt = 1_000L)
        val picked = resolveAccountForUrl(
            accounts = listOf(signedOut, signedIn),
            activeAccountId = signedOut.id,
            absoluteUrl = "$url/api/attachments/xyz",
        )
        assertEquals(signedIn.id, picked?.id)
    }

    @Test
    fun foreignHostAndPrefixLookalikeNeverMatch() {
        val acct = account("A")
        assertNull(
            resolveAccountForUrl(listOf(acct), acct.id, "https://team.example.evil/api/attachments/xyz"),
        )
        assertNull(
            resolveAccountForUrl(listOf(acct), acct.id, "https://other.example/api/attachments/xyz"),
        )
    }

    @Test
    fun bareInstanceUrlAndTrailingSlashStillMatch() {
        val acct = account("A", instanceUrl = "$url/")
        val picked = resolveAccountForUrl(listOf(acct), null, "$url/api/attachments/xyz")
        assertEquals(acct.id, picked?.id)
        assertEquals(acct.id, resolveAccountForUrl(listOf(acct), null, url)?.id)
    }
}
