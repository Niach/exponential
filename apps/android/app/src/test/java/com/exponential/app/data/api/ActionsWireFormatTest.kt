package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-694: wire-format lock for the mobile action editor's `actions.update`.
 * The router applies a field only when the key is present (`!== undefined`),
 * so "clear the description/icon/repository" HAS to travel as a literal
 * `null` — and the shared Json's explicitNulls=false would drop a null
 * property of a `@Serializable` class, turning every clear into a silent no-op
 * (the DevicesWireFormatTest story for `devices.setShared`).
 */
class ActionsWireFormatTest {

    // Mirrors HttpClientProvider's shared Json — the one TrpcClient encodes with.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `cleared fields emit literal nulls`() {
        assertEquals(
            """{"id":"a-1","name":"Ship it","description":null,"icon":null,""" +
                """"repositoryId":null,"body":"Do the thing"}""",
            updateActionInput(
                id = "a-1",
                name = "Ship it",
                description = null,
                icon = null,
                repositoryId = null,
                body = "Do the thing",
            ).toString(),
        )
    }

    @Test
    fun `set fields ride as their values`() {
        assertEquals(
            """{"id":"a-1","name":"Ship it","description":"One-liner","icon":"rocket",""" +
                """"repositoryId":"repo-1","body":"Do the thing"}""",
            updateActionInput(
                id = "a-1",
                name = "Ship it",
                description = "One-liner",
                icon = "rocket",
                repositoryId = "repo-1",
                body = "Do the thing",
            ).toString(),
        )
    }

    @Test
    fun `the shared Json keeps the nulls through encoding`() {
        val encoded = json.encodeToString(
            JsonObject.serializer(),
            updateActionInput(
                id = "a-1",
                name = "Ship it",
                description = null,
                icon = null,
                repositoryId = null,
                body = "",
            ),
        )
        assertEquals(
            """{"id":"a-1","name":"Ship it","description":null,"icon":null,""" +
                """"repositoryId":null,"body":""}""",
            encoded,
        )
    }
}
