package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-1154: the Close PR copy is byte-identical x4 — this reads the shared
// `packages/domain-contract/fixtures/close-pr.json` (web, desktop and iOS read
// the same file).
class ClosePrCopyTest {
    private val fixture: JsonObject = run {
        val candidates = listOf(
            "../../../packages/domain-contract/fixtures/close-pr.json",
            "../../packages/domain-contract/fixtures/close-pr.json",
            "packages/domain-contract/fixtures/close-pr.json",
        )
        val file = candidates.map(::File).firstOrNull { it.isFile }
            ?: error("close-pr.json not found from ${File(".").absolutePath}")
        Json.parseToJsonElement(file.readText()).jsonObject
    }

    private fun copy(key: String) = fixture[key]!!.jsonPrimitive.content

    @Test
    fun `the menu item and the confirm match the fixture`() {
        assertEquals(copy("menuItem"), DomainContract.diffUiClosePr)
        assertEquals(copy("title"), ClosePr.TITLE)
        assertEquals(copy("body"), ClosePr.BODY)
        assertEquals(copy("batchLine"), ClosePr.BATCH_LINE)
        assertEquals(copy("confirm"), ClosePr.CONFIRM)
    }

    @Test
    fun `a batch pull request appends the linked issues line`() {
        assertEquals(copy("body"), ClosePr.body(0))
        assertEquals(
            copy("body") + " " + copy("batchLine").replace("{n}", "2"),
            ClosePr.body(2),
        )
    }
}
