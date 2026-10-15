package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * EXP-1324: the CodeBlock's `ansi` language, locked against the `ansi` cases
 * of `packages/exponential-ui/fixtures/code-tokens.json` (the eUI built-in
 * tokenizer's fixture) — every line's whole token list (iOS `AnsiCodeTests`).
 */
class AnsiCodeTest {

    private fun fixture(): String {
        val candidates = listOf(
            "../../../packages/exponential-ui/fixtures/code-tokens.json",
            "../../packages/exponential-ui/fixtures/code-tokens.json",
            "packages/exponential-ui/fixtures/code-tokens.json",
        )
        return candidates.map(::File).first { it.isFile }.readText()
    }

    @Test
    fun `the ansi cases tokenize line by line`() {
        val cases = Json.parseToJsonElement(fixture()).jsonObject.getValue("cases").jsonArray
            .map { it.jsonObject }
            .filter { it.getValue("language").jsonPrimitive.content == "ansi" }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case.getValue("name").jsonPrimitive.content
            val code = case.getValue("code").jsonPrimitive.content
            val expected = case.getValue("expected").jsonArray.map { line ->
                line.jsonArray.map {
                    val token = it.jsonObject
                    CodeToken(token.getValue("kind").jsonPrimitive.content, token.getValue("text").jsonPrimitive.content)
                }
            }
            val got = AnsiCode.tokenize(code)
            assertEquals(name, expected, got)
            // The shown + copied text = every line's tokens, escape-free.
            assertEquals(name, got.joinToString("\n") { line -> line.joinToString("") { it.text } }, AnsiCode.text(code))
        }
    }
}
