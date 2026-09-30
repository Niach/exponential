package com.exponential.app.data.auth

import java.net.URLDecoder
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-1126: the oauth-return deep link's three answers (error / linked /
 * code) and the link-failure copy, which must match the web's
 * `oauthLinkErrorMessage` byte-for-byte.
 */
class OauthReturnTest {

    // URI-style percent decoding (`+` stays literal), like android.net.Uri.decode.
    private val decode: (String) -> String = { URLDecoder.decode(it.replace("+", "%2B"), "UTF-8") }

    @Test
    fun codeFromTheFragment() {
        assertEquals(OauthReturn.Result.Code("abc"), OauthReturn.parse("code=abc", "code=abc", decode))
    }

    @Test
    fun codeFallsBackToTheQueryWhenTheFragmentWasDropped() {
        assertEquals(OauthReturn.Result.Code("abc"), OauthReturn.parse(null, "code=abc", decode))
    }

    @Test
    fun fragmentWinsOverTheQuery() {
        assertEquals(OauthReturn.Result.Code("frag"), OauthReturn.parse("code=frag", "code=query", decode))
    }

    @Test
    fun linkedCarriesTheProviderId() {
        assertEquals(
            OauthReturn.Result.Linked("google"),
            OauthReturn.parse("linked=google", "linked=google", decode),
        )
        assertEquals(OauthReturn.Result.Linked("my-oidc"), OauthReturn.parse(null, "linked=my-oidc", decode))
    }

    @Test
    fun errorBeatsLinkedBeatsCode() {
        assertEquals(
            OauthReturn.Result.Error("access_denied"),
            OauthReturn.parse("linked=google&code=x", "error=access_denied", decode),
        )
        assertEquals(OauthReturn.Result.Linked("apple"), OauthReturn.parse("code=x&linked=apple", null, decode))
    }

    @Test
    fun valuesAreDecodedUriStyle() {
        assertEquals(OauthReturn.Result.Code("a+b c"), OauthReturn.parse("code=a+b%20c", null, decode))
    }

    @Test
    fun emptyOrMissingIsNone() {
        assertEquals(OauthReturn.Result.None, OauthReturn.parse(null, null, decode))
        assertEquals(OauthReturn.Result.None, OauthReturn.parse("code=", "other=1", decode))
    }

    @Test
    fun linkErrorCopyMatchesTheWeb() {
        assertEquals("Linking was cancelled.", OauthReturn.linkErrorMessage("access_denied"))
        listOf(
            "link_ticket_invalid", "state_missing", "state_invalid", "state_mismatch", "state_not_found",
            "please_restart_the_process",
        ).forEach {
            assertEquals("That link request expired. Please try again.", OauthReturn.linkErrorMessage(it))
        }
        assertEquals(
            "That account is already linked to a different user.",
            OauthReturn.linkErrorMessage("account_already_linked_to_different_user"),
        )
        assertEquals(
            "Couldn't link that account. Please try again.",
            OauthReturn.linkErrorMessage("unable_to_link_account"),
        )
        assertEquals("Couldn't link that account. Please try again.", OauthReturn.linkErrorMessage("whatever"))
    }
}
