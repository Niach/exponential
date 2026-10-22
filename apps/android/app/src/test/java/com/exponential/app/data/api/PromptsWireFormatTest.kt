package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * NEW-2: wire-format lock for the saved-prompt writes (`prompts.create` /
 * `prompts.update`, strict server-side: the retired action fields `inputs`,
 * `description` and `promptPlaceholder` must never travel). The router
 * applies a field only when the key is present, so "clear the icon" HAS to
 * travel as a literal `null` — the shared Json's explicitNulls=false would
 * drop a null property of a `@Serializable` class.
 */
class PromptsWireFormatTest {

    // Mirrors HttpClientProvider's shared Json — the one TrpcClient encodes with.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private val fields = PromptFields(name = "  Release notes ", icon = "", body = "Write them.")

    @Test
    fun `create trims the name and omits a blank icon`() {
        val input = createPromptInput("team-1", fields)
        assertEquals("""{"teamId":"team-1","name":"Release notes","body":"Write them."}""", input.toString())
        assertFalse("icon" in input)
        assertEquals(
            """{"teamId":"team-1","name":"Ship","icon":"rocket","body":"Go"}""",
            createPromptInput("team-1", PromptFields("Ship", "rocket", "Go")).toString(),
        )
    }

    @Test
    fun `update clears a blank icon with an explicit null`() {
        val input = updatePromptInput("p-1", fields)
        assertEquals(JsonNull, input["icon"])
        assertEquals(
            """{"id":"p-1","name":"Release notes","icon":null,"body":"Write them."}""",
            json.encodeToString(JsonObject.serializer(), input),
        )
        assertEquals(
            """{"id":"p-1","name":"Ship","icon":"rocket","body":"Go"}""",
            updatePromptInput("p-1", PromptFields("Ship", "rocket", "Go")).toString(),
        )
    }

    @Test
    fun `no retired action field ever travels`() {
        for (input in listOf(createPromptInput("t", fields), updatePromptInput("p", fields))) {
            for (key in listOf("inputs", "description", "promptPlaceholder")) assertFalse(key, key in input)
        }
    }

    @Test
    fun `the prompt row decodes with or without its body`() {
        val full = json.decodeFromString(
            PromptResult.serializer(),
            """{"prompt":{"id":"p-1","teamId":"t-1","name":"Release","icon":null,"body":"Ship",""" +
                """"sortOrder":1,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}}""",
        ).prompt
        assertEquals("Ship", full.body)
        assertEquals(null, full.icon)
        val bare = json.decodeFromString(PromptDto.serializer(), """{"id":"p-1","teamId":"t-1","name":"Release"}""")
        assertEquals("", bare.body)
        assertEquals(0.0, bare.sortOrder, 0.0)
    }
}
