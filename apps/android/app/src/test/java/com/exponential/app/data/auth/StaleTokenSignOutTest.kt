package com.exponential.app.data.auth

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The sign-out race (#ERR-19 release smoke): a 401 answering a request that
 * left with the OLD bearer (a shape poll, an in-flight tRPC call) arrives
 * after the user signed in again. Compare-and-clear: only the token the
 * server rejected is ever cleared, never the one the user just signed in with.
 */
class StaleTokenSignOutTest {

    /** SecureStore over a map; counts the writes so the one-edit rule is checkable. */
    private class MemoryStore : SecureStore({ error("no prefs in a unit test") }) {
        val values = mutableMapOf<String, String>()
        var edits = 0
        override fun get(key: String): String? = values[key]
        override fun set(key: String, value: String?) {
            edits++
            if (value == null) values.remove(key) else values[key] = value
        }
        override fun setAll(values: Map<String, String?>) {
            edits++
            for ((key, value) in values) if (value == null) this.values.remove(key) else this.values[key] = value
        }
    }

    private fun signedIn(store: MemoryStore, token: String): Pair<AccountStore, String> {
        val accounts = AccountStore(store)
        accounts.upsertAndActivate("https://example.test")
        accounts.resolveActiveAccount(
            token = token,
            email = "a@example.test",
            name = null,
            userId = "user-1",
            isAdmin = false,
            onboardingCompletedAt = null,
            onboardingKnown = false,
        )
        return accounts to accounts.activeAccountId.value!!
    }

    @Test
    fun `a stale 401 after a re-login keeps the new token`() {
        val store = MemoryStore()
        val (accounts, id) = signedIn(store, "old-token")
        // The user signs in again while a poll carrying the OLD bearer is in flight.
        accounts.resolveActiveAccount("new-token", "a@example.test", null, "user-1", false, null, false)
        val held = accounts.accounts.value.first { it.id == id }.token
        assertFalse(SessionInvalidation.shouldClear(held, presented = "old-token"))
        // Even a direct compare-and-clear with the old token is a no-op.
        accounts.clearToken(id, expected = "old-token")
        assertEquals("new-token", accounts.accounts.value.first { it.id == id }.token)
        assertEquals("new-token", AccountStore(store).accounts.value.first { it.id == id }.token)
    }

    @Test
    fun `a 401 on the token the account holds still signs it out`() {
        val store = MemoryStore()
        val (accounts, id) = signedIn(store, "live-token")
        val held = accounts.accounts.value.first { it.id == id }.token
        assertTrue(SessionInvalidation.shouldClear(held, presented = "live-token"))
        accounts.clearToken(id, expected = "live-token")
        assertNull(accounts.accounts.value.first { it.id == id }.token)
        // The account row stays (the login screen keeps its server).
        assertEquals(id, AccountStore(store).activeAccountId.value)
        assertNull(AccountStore(store).accounts.value.first { it.id == id }.token)
    }

    @Test
    fun `no bearer presented never signs anyone out`() {
        assertFalse(SessionInvalidation.shouldClear(held = "t", presented = null))
        assertFalse(SessionInvalidation.shouldClear(held = null, presented = "t"))
    }

    @Test
    fun `the explicit sign-out clears whatever token the account holds`() {
        val store = MemoryStore()
        val (accounts, id) = signedIn(store, "any-token")
        accounts.clearToken(id)
        assertNull(accounts.accounts.value.first { it.id == id }.token)
    }

    @Test
    fun `the accounts and the active id persist in one edit`() {
        val store = MemoryStore()
        val (accounts, id) = signedIn(store, "t")
        val before = store.edits
        accounts.setActive(id)
        assertEquals("one write per persist (accounts + active id together)", before + 1, store.edits)
        val reloaded = AccountStore(store)
        assertEquals(id, reloaded.activeAccountId.value)
        assertEquals("t", reloaded.accounts.value.first { it.id == id }.token)
    }
}
