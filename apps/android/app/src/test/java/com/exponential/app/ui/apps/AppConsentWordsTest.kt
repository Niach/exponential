package com.exponential.app.ui.apps

import at.exponential.ui.host.FunctionCallInfo
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** VAPP-107: what the app host's consent card shows of a call (web
 *  `consentWords`, desktop `consent_words`). */
class AppConsentWordsTest {
    private fun call(name: String, args: Map<String, JsonValue>) = FunctionCallInfo("s", "c", name, args)

    @Test
    fun mcpCallShowsItsToolAndArguments() {
        val words = AppConsentWords.of(
            call(
                "harness.mcp",
                mapOf(
                    "tool" to JsonValue.Str("exponential_issues"),
                    "arguments" to JsonValue.Obj(mapOf("op" to JsonValue.Str("delete"), "id" to JsonValue.Str("EXP-1"))),
                ),
            ),
        )
        assertEquals("exponential_issues", words.tool)
        assertEquals("""{"op":"delete","id":"EXP-1"}""", words.args)
        assertTrue(words.destructive)
    }

    @Test
    fun emptyArgumentsShowNoBlock() {
        val words = AppConsentWords.of(call("harness.toast", emptyMap()))
        assertEquals("harness.toast", words.tool)
        assertNull(words.args)
        assertEquals(false, words.destructive)
    }

    @Test
    fun longArgumentsPreviewAtTheCap() {
        val long = "x".repeat(400)
        val words = AppConsentWords.of(call("harness.toast", mapOf("message" to JsonValue.Str(long))))
        assertEquals(AppConsentWords.PREVIEW_CHARS + 1, words.argsPreview!!.length)
        assertTrue(words.argsPreview!!.endsWith("…"))
    }
}
