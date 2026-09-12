package com.exponential.app.ui.auth

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-857: the passkey relying-party check. The assertion options come from
 * whichever instance the app is pointed at, and the ceremony signs the `rpId`
 * they name — so a hostile instance URL must never be able to relay a
 * challenge for someone else's host.
 */
class PasskeyRpIdTest {

    private fun options(rpId: String?): String =
        if (rpId == null) {
            """{"challenge":"abc","timeout":60000,"userVerification":"preferred"}"""
        } else {
            """{"challenge":"abc","rpId":"$rpId","userVerification":"preferred"}"""
        }

    @Test
    fun `the instance's own relying party is accepted`() {
        assertTrue(
            passkeyRpIdMatchesInstance(
                "https://app.exponential.at",
                options("app.exponential.at"),
            ),
        )
        // Host casing and a trailing root dot are the same host.
        assertTrue(
            passkeyRpIdMatchesInstance(
                "https://App.Exponential.at/",
                options("app.exponential.at."),
            ),
        )
        // A self-hosted instance on a port: the port is not part of the rpId.
        assertTrue(
            passkeyRpIdMatchesInstance(
                "https://exp.acme.test:3000",
                options("exp.acme.test"),
            ),
        )
    }

    @Test
    fun `a challenge for another host is refused`() {
        // The attack: a hostile instance relays app.exponential.at's challenge
        // and replays the assertion it gets back.
        assertFalse(
            passkeyRpIdMatchesInstance(
                "https://evil.test",
                options("app.exponential.at"),
            ),
        )
        // A registrable suffix is a mismatch too — our server never sends one.
        assertFalse(
            passkeyRpIdMatchesInstance(
                "https://app.exponential.at",
                options("exponential.at"),
            ),
        )
        // A subdomain of the instance is not the instance.
        assertFalse(
            passkeyRpIdMatchesInstance(
                "https://exponential.at",
                options("app.exponential.at"),
            ),
        )
    }

    @Test
    fun `anything unreadable is a mismatch`() {
        val url = "https://app.exponential.at"
        assertFalse(passkeyRpIdMatchesInstance(url, options(null)))
        assertFalse(passkeyRpIdMatchesInstance(url, options("")))
        assertFalse(passkeyRpIdMatchesInstance(url, options("   ")))
        assertFalse(passkeyRpIdMatchesInstance(url, """{"rpId":{"host":"app.exponential.at"}}"""))
        assertFalse(passkeyRpIdMatchesInstance(url, """{"rpId":null}"""))
        assertFalse(passkeyRpIdMatchesInstance(url, "not json"))
        // No usable host on our side either.
        assertFalse(passkeyRpIdMatchesInstance("", options("app.exponential.at")))
        assertFalse(passkeyRpIdMatchesInstance("not a url", options("app.exponential.at")))
    }
}
