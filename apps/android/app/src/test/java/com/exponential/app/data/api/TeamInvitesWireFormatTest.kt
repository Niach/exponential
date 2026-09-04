package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-725: wire-format lock for the invite-link creator's two calls.
 *
 * `teamInvites.create` defaults `role` server-side, but the app sends it
 * explicitly — the wire shape IS the contract, and a member invite is the only
 * kind any client mints. The result carries the invite row and
 * `emailDelivered` too; only `token` is decoded, so the decode must tolerate
 * the rest.
 *
 * `teams.inviteCapacity` answers `remaining: number | null`, and the two are
 * NOT the same thing: null means UNLIMITED (self-hosted, or a paid/comp tier)
 * while 0 means the control disappears. A `remaining` that decoded null when
 * the server said 0 would keep offering invites the server refuses; one that
 * decoded 0 for null would hide the creator on every self-hosted instance.
 */
class TeamInvitesWireFormatTest {

    // Mirrors HttpClientProvider's shared Json — the one TrpcClient encodes
    // and decodes with.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `create names the team and an explicit member role`() {
        assertEquals(
            """{"teamId":"team-1","role":"member"}""",
            json.encodeToString(CreateInviteInput.serializer(), CreateInviteInput(teamId = "team-1")),
        )
    }

    @Test
    fun `create decodes only the token off the full result`() {
        val body = """{"invite":{"id":"inv-1","teamId":"team-1","role":"member",""" +
            """"expiresAt":"2026-09-11T00:00:00.000Z","createdAt":"2026-09-04T00:00:00.000Z"},""" +
            """"token":"tok123","emailDelivered":null}"""
        assertEquals(
            "tok123",
            json.decodeFromString(CreateInviteResult.serializer(), body).token,
        )
    }

    @Test
    fun `inviteCapacity asks about one team`() {
        assertEquals(
            """{"teamId":"team-1"}""",
            json.encodeToString(InviteCapacityInput.serializer(), InviteCapacityInput(teamId = "team-1")),
        )
    }

    @Test
    fun `an unlimited capacity decodes as null, not zero`() {
        assertNull(
            json.decodeFromString(InviteCapacityResult.serializer(), """{"remaining":null}""").remaining,
        )
        // …and an absent key means the same thing.
        assertNull(
            json.decodeFromString(InviteCapacityResult.serializer(), """{}""").remaining,
        )
    }

    @Test
    fun `a numeric capacity decodes verbatim`() {
        assertEquals(
            1,
            json.decodeFromString(InviteCapacityResult.serializer(), """{"remaining":1}""").remaining,
        )
        // The value that REMOVES the creator entirely (store billing policy).
        assertEquals(
            0,
            json.decodeFromString(InviteCapacityResult.serializer(), """{"remaining":0}""").remaining,
        )
    }
}
