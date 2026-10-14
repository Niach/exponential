package com.exponential.app.ui.actions

import com.exponential.app.data.api.ActionInputDto
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ActionWritesTest {

    private val fields = ActionFields(
        name = "  Release notes ",
        icon = "",
        body = "Write them.",
        inputs = listOf(ActionInputDto(key = "issue", label = " Issue ", type = "issue", required = true)),
        promptPlaceholder = "  ",
    )

    @Test
    fun createOmitsBlankIconAndHint() {
        val json = createActionInput("team-1", fields)
        assertEquals("Release notes", json["name"]!!.jsonPrimitive.content)
        assertFalse("icon" in json)
        assertFalse("promptPlaceholder" in json)
        val input = json["inputs"]!!.jsonArray.single().jsonObject
        assertEquals("Issue", input["label"]!!.jsonPrimitive.content)
        assertEquals("issue", input["type"]!!.jsonPrimitive.content)
        assertFalse("placeholder" in input)
    }

    @Test
    fun updateClearsBlankIconAndHintWithExplicitNull() {
        val json = updateActionFieldsInput("a-1", fields)
        assertEquals(JsonNull, json["icon"])
        assertEquals(JsonNull, json["promptPlaceholder"])
        assertEquals(1, json["inputs"]!!.jsonArray.size)
    }

    @Test
    fun inputKeysAreSnakeCaseAndUnique() {
        assertEquals("pull_request", actionInputKey("Pull request", emptySet()))
        assertEquals("issue_2", actionInputKey("Issue", setOf("issue")))
        assertEquals("input", actionInputKey("123 !!", emptySet()))
    }
}
