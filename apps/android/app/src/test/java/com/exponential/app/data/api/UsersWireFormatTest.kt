package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1126: `users.signInMethods` + the link ticket decode, and the last-way-in rule. */
class UsersWireFormatTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private val payload = """
        {"email":"a@b.co","emailVerified":true,"emailOtpEnabled":true,"passwordEnabled":true,
         "passkeyEnabled":true,
         "providers":[
           {"id":"apple","name":"Apple","kind":"apple","available":true,"linked":false,"linkedAt":null},
           {"id":"google","name":"Google","kind":"google","available":true,"linked":true,
            "linkedAt":"2026-09-01T10:00:00.000Z"},
           {"id":"old-oidc","name":"old-oidc","kind":"oidc","available":false,"linked":true,"linkedAt":null},
           {"id":"credential","name":"Password","kind":"password","available":true,"linked":true,
            "linkedAt":"2026-01-01T00:00:00.000Z","extra":1}
         ],
         "passkeys":[{"id":"pk1","name":null,"createdAt":"2026-09-02T00:00:00.000Z","backedUp":true}],
         "waysIn":4,"future":"ignored"}
    """.trimIndent()

    @Test
    fun decodesTheSignInMethodsPayload() {
        val m = json.decodeFromString(SignInMethodsDto.serializer(), payload)
        assertEquals("a@b.co", m.email)
        assertTrue(m.emailOtpEnabled)
        assertEquals(listOf("apple", "google", "old-oidc", "credential"), m.providers.map { it.id })
        assertEquals("password", m.providers.last().kind)
        assertNull(m.providers.first().linkedAt)
        assertFalse(m.providers[2].available)
        assertNull(m.passkeys.single().name)
        assertTrue(m.passkeys.single().backedUp)
        assertEquals(4, m.waysIn)
    }

    @Test
    fun decodesTheLinkTicket() {
        val t = json.decodeFromString(SignInLinkTicketDto.serializer(), """{"ticket":"tk","expiresInSeconds":120}""")
        assertEquals("tk", t.ticket)
        assertEquals(120, t.expiresInSeconds)
    }

    @Test
    fun mutationInputsNameTheServerFields() {
        assertEquals(
            """{"providerId":"google"}""",
            json.encodeToString(UnlinkSignInMethodInput.serializer(), UnlinkSignInMethodInput("google")),
        )
        assertEquals("""{"id":"pk1"}""", json.encodeToString(DeletePasskeyInput.serializer(), DeletePasskeyInput("pk1")))
        assertEquals(
            """{"provider":"apple"}""",
            json.encodeToString(MintSignInLinkTicketInput.serializer(), MintSignInLinkTicketInput("apple")),
        )
    }

    @Test
    fun canUnlinkOnlyALinkedRowWhileAnotherWayInRemains() {
        val m = json.decodeFromString(SignInMethodsDto.serializer(), payload)
        val apple = m.providers[0]
        val google = m.providers[1]
        assertFalse(canUnlink(m, apple))
        assertTrue(canUnlink(m, google))
        val last = m.copy(waysIn = 1)
        assertFalse(canUnlink(last, google))
        assertFalse(canRemovePasskey(last))
        assertTrue(canRemovePasskey(m))
    }

    @Test
    fun linkStartUrlUsesProviderForSocialAndProviderIdForOidc() {
        assertEquals(
            "https://x.test/api/mobile-oauth-start?link=t.1&provider=google&code_challenge=ch",
            AuthWire.linkStartUrl("https://x.test/", "t.1", "google", "ch"),
        )
        assertEquals(
            "https://x.test/api/mobile-oauth-start?link=t%2B1&providerId=my+oidc&code_challenge=ch",
            AuthWire.linkStartUrl("https://x.test", "t+1", "my oidc", "ch"),
        )
    }
}
