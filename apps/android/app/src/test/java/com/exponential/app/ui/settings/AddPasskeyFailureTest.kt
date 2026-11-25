package com.exponential.app.ui.settings

import androidx.credentials.exceptions.CreateCredentialCancellationException
import androidx.credentials.exceptions.CreateCredentialNoCreateOptionException
import androidx.credentials.exceptions.CreateCredentialUnknownException
import com.exponential.app.data.api.AuthWire
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** EXP-1637: every failed Add passkey is a toast that says why. */
class AddPasskeyFailureTest {
    @Test
    fun `a stale session says to sign in again`() {
        val message = AuthWire.passkeyRegisterErrorMessage("SESSION_NOT_FRESH", "Session is not fresh")
        assertEquals("Sign in again to add a passkey (your current session is older than a day).", message)
        val failure = addPasskeyFailure(IllegalStateException(message))
        assertEquals("add", failure.verb)
        assertEquals("the passkey", failure.thing)
        assertEquals(message, failure.message)
    }

    @Test
    fun `another refusal keeps the server's message`() {
        assertEquals("Nope", AuthWire.passkeyRegisterErrorMessage("OTHER", "Nope"))
        assertNull(AuthWire.passkeyRegisterErrorMessage(null, null))
    }

    @Test
    fun `a cancellation is never silent`() {
        assertEquals("Passkey registration was cancelled.", addPasskeyFailure(CreateCredentialCancellationException()).message)
    }

    @Test
    fun `no provider says what to set up`() {
        assertEquals(
            "No passkey provider is available. Set up a password manager in your phone's settings.",
            addPasskeyFailure(CreateCredentialNoCreateOptionException()).message,
        )
    }

    @Test
    fun `framework jargon and blanks leave the title alone`() {
        assertNull(addPasskeyFailure(CreateCredentialUnknownException("[16] jargon")).message)
        assertNull(addPasskeyFailure(IllegalStateException("")).message)
    }
}
