package com.exponential.app.data.auth

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The one decision in the app that clears a session token, pinned narrow: a
 * dead session must sign the account out (never a screen that 401s forever),
 * and everything else must leave a working account alone. Mirrors the desktop's
 * `status_error_authed` tests (crates/api/src/error.rs).
 */
class SessionInvalidationTest {

    private fun status(code: Int, tokenPresented: Boolean = true) =
        SessionInvalidation.classifyStatus(code, tokenPresented)

    private fun sessionRead(code: Int, tokenPresented: Boolean = true, hasUser: Boolean = false) =
        SessionInvalidation.classifySessionRead(code, tokenPresented, hasUser)

    @Test
    fun `a rejected bearer invalidates the session`() {
        assertEquals(SessionSignal.Invalidated, status(401))
    }

    @Test
    fun `forbidden never signs anyone out`() {
        // A live session without access to something (wrong team, non-owner).
        // The shape loops raise the same auth failure for 401 and 403, so this
        // is the split that keeps a 403 from ejecting a working account.
        assertEquals(SessionSignal.Inconclusive, status(403))
    }

    @Test
    fun `server and gateway failures are not a dead session`() {
        for (code in listOf(400, 404, 409, 426, 429, 500, 502, 503, 504)) {
            assertEquals("HTTP $code", SessionSignal.Inconclusive, status(code))
        }
    }

    @Test
    fun `a 401 without a bearer is bad credentials`() {
        // Sign-in and auth-config calls carry no token; their 401 says "wrong
        // password", and acting on it would sign out an unrelated account.
        assertEquals(SessionSignal.Inconclusive, status(401, tokenPresented = false))
    }

    @Test
    fun `a session read that returns no user for a presented token is dead`() {
        // Better Auth answers a dead bearer with 200 + a null session, which is
        // why status alone is not enough to spot a deleted account.
        assertEquals(SessionSignal.Invalidated, sessionRead(200, hasUser = false))
        assertEquals(SessionSignal.Invalidated, sessionRead(204, hasUser = false))
    }

    @Test
    fun `a session read that returns a user is alive`() {
        assertEquals(SessionSignal.Inconclusive, sessionRead(200, hasUser = true))
    }

    @Test
    fun `a session read rejected with 401 is dead`() {
        assertEquals(SessionSignal.Invalidated, sessionRead(401, hasUser = false))
    }

    @Test
    fun `a failed session read proves nothing`() {
        // The conservative half of the bug: "offline" used to be
        // indistinguishable from "account deleted".
        for (code in listOf(429, 500, 502, 503, 504)) {
            assertEquals("HTTP $code", SessionSignal.Inconclusive, sessionRead(code))
        }
    }

    @Test
    fun `an anonymous session read is never invalidating`() {
        // No token was presented, so "no user" is simply the truth about an
        // anonymous request.
        assertEquals(
            SessionSignal.Inconclusive,
            sessionRead(200, tokenPresented = false, hasUser = false),
        )
    }
}
