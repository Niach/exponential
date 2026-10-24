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
    fun typedActionIsWordedForItsOperation() {
        // VAPP-120: `actionConsent` words a typed action; its title replaces the generic prompt's.
        val words = AppConsentWords.of(
            call("exponential.issues.delete", mapOf("id" to JsonValue.Str("APP-5"))),
        )
        assertEquals("exponential_issues", words.tool)
        assertEquals("""{"id":"APP-5"}""", words.args)
        assertTrue(words.destructive)
        assertEquals("Allow this surface to delete issue APP-5?", words.title)

        val plain = AppConsentWords.of(
            call("exponential.issues.update_status", mapOf("id" to JsonValue.Str("APP-5"), "status" to JsonValue.Str("done"))),
        )
        assertEquals("Allow this surface to set issue status APP-5?", plain.title)
        assertEquals("""{"id":"APP-5","status":"done"}""", plain.args)
        assertEquals(false, plain.destructive)
    }

    @Test
    fun mcpKeepsTheGenericCopy() {
        val words = AppConsentWords.of(
            call("harness.mcp", mapOf("tool" to JsonValue.Str("exponential_issues"), "arguments" to JsonValue.Obj(mapOf("op" to JsonValue.Str("get"))))),
        )
        assertNull(words.title)
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
