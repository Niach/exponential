package com.exponential.app.data.api

import com.exponential.app.domain.EmailCodeSignUpCopy
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1026: the email-code sign-up name step. The copy is byte-identical with
 * iOS `EmailCodeSignUpCopy` (`EmailCodeSignUpTests` locks the same literals);
 * the wire contract is the one the server pins: `X-Exp-Ask-Name: 1` on every
 * verify, 400 `NAME_REQUIRED` for an unknown address sent without a name, then
 * `{email, otp, name}`.
 */
class EmailCodeSignUpTest {

    @Test
    fun copyLiterals() {
        assertEquals("What should we call you?", EmailCodeSignUpCopy.TITLE)
        assertEquals("This is how your teammates see you.", EmailCodeSignUpCopy.BODY)
        assertEquals("Name", EmailCodeSignUpCopy.FIELD_LABEL)
        assertEquals("Your name", EmailCodeSignUpCopy.PLACEHOLDER)
        assertEquals("Create account", EmailCodeSignUpCopy.BUTTON)
        assertEquals("Enter your name to continue.", EmailCodeSignUpCopy.EMPTY_NAME_ERROR)
    }

    @Test
    fun askNameHeader() {
        assertEquals("X-Exp-Ask-Name", EmailCodeSignUp.ASK_NAME_HEADER)
        assertEquals("1", EmailCodeSignUp.ASK_NAME_VALUE)
    }

    @Test
    fun requestBodyCarriesTheNameOnlyOnceGiven() {
        val bare = buildJsonObject {
            put("email", JsonPrimitive("a@b.c"))
            put("otp", JsonPrimitive("123456"))
        }
        assertEquals(bare, EmailCodeSignUp.requestBody("a@b.c", "123456", null))
        assertEquals(bare, EmailCodeSignUp.requestBody("a@b.c", "123456", "   "))
        assertEquals(
            buildJsonObject {
                put("email", JsonPrimitive("a@b.c"))
                put("otp", JsonPrimitive("123456"))
                put("name", JsonPrimitive("Ada Lovelace"))
            },
            EmailCodeSignUp.requestBody("a@b.c", "123456", "  Ada Lovelace "),
        )
    }

    @Test
    fun nameRequiredIsOnlyThe400Code() {
        assertTrue(EmailCodeSignUp.isNameRequired(400, "NAME_REQUIRED"))
        assertFalse(EmailCodeSignUp.isNameRequired(400, "INVALID_OTP"))
        assertFalse(EmailCodeSignUp.isNameRequired(401, "NAME_REQUIRED"))
        assertFalse(EmailCodeSignUp.isNameRequired(400, null))
    }
}
