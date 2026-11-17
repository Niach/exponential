package com.exponential.app.data.sealed

import java.io.File
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** #EDE-97: packages/domain-contract/fixtures/sealed-content.json `clearTree` replayed through [sendWire] + [StaleTree]. */
class ClearTreeFixtureTest {
    private val clearTree: JsonObject = run {
        val path = System.getProperty("exponential.enet.sealedFixture")
            ?: error("exponential.enet.sealedFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject.getValue("clearTree").jsonObject
    }

    private fun JsonObject.str(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.flag(key: String) = this[key]?.jsonPrimitive?.boolean ?: false

    private val root = "00000000-0000-4000-8000-0000000000cc"

    @Test
    fun theCopyAndWaitsAreTheFixtures() {
        assertEquals(clearTree.str("noKeyRefusal"), NO_KEY_REFUSAL)
        assertEquals(clearTree.str("clearRefusal"), CLEAR_TREE_REFUSAL)
        assertEquals(clearTree.getValue("holderWaitMs").jsonPrimitive.long, SEAL_HOLDER_WAIT_MS)
        assertEquals(clearTree.getValue("keyWaitMs").jsonPrimitive.long, SEAL_KEY_WAIT_MS)
    }

    @Test
    fun writerCasesReplayTheFixture() {
        val cases = clearTree.getValue("writerCases").jsonArray
        assertEquals(12, cases.size)
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.str("name")
            val tree = SendTree.entries.single { it.name.lowercase() == case.str("tree") }
            val holder = SendHolder.entries.single { it.name.lowercase() == case.str("holder") }
            val wire = sendWire(tree, holder, reader = case.flag("reader"), keyArrives = case.flag("keyArrives"))
            assertEquals(name, case.str("wire"), wire.name.lowercase())
        }
    }

    /** A clear root (Room's or the server's word) resolves to plaintext, never a key lookup or create. */
    @Test
    fun aClearRootResolvesClear() = runBlocking {
        val local = StaleTree.resolve(
            conversation = false,
            explicitId = root,
            local = LocalRoot(root, null, clear = true),
            fresh = false,
            sendTarget = { error("no sendTarget for a session") },
            publicKey = { error("a clear tree has no key") },
        )
        assertEquals(TreeWire.Clear(root), local)
        val server = StaleTree.resolve(
            conversation = true,
            explicitId = null,
            local = null,
            fresh = false,
            sendTarget = { ServerTree(root, null, clear = true) },
            publicKey = { error("a clear tree has no key") },
        )
        assertEquals(TreeWire.Clear(root), server)
    }

    @Test
    fun theWaitAnswersOnceTheProbeDoesOrGivesUp() = runBlocking {
        var probes = 0
        assertEquals("key", awaitWithin(1_000, 1) { if (++probes < 3) null else "key" })
        assertEquals(3, probes)
        assertNull(awaitWithin<String>(30, 5) { null })
    }
}
