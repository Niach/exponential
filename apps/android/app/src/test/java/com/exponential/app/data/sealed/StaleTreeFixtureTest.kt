package com.exponential.app.data.sealed

import com.exponential.app.data.api.TrpcException
import java.io.File
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** #FEED-250: packages/domain-contract/fixtures/sealed-content.json `staleTree` replayed through [StaleTree]. */
class StaleTreeFixtureTest {
    private val staleTree: JsonObject = run {
        val path = System.getProperty("exponential.enet.sealedFixture")
            ?: error("exponential.enet.sealedFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject.getValue("staleTree").jsonObject
    }

    private fun JsonObject.str(key: String) = getValue(key).jsonPrimitive.content

    private val root = "00000000-0000-4000-8000-0000000000aa"
    private val serverKey = PublicTreeKey("server-key", 3)
    private val localKey = PublicTreeKey("local-key", 3)

    @Test
    fun theRefusalIsTheFixtures() {
        assertEquals(staleTree.str("refusal"), STALE_TREE_REFUSAL)
        assertTrue(isStaleTreeRefusal(STALE_TREE_REFUSAL))
        assertTrue(isStaleTreeRefusal("The tree is at epoch 1"))
        assertFalse(isStaleTreeRefusal("The tree is at epoch one"))
        assertFalse(isStaleTreeRefusal("Not found"))
        assertFalse(isStaleTreeRefusal(null))
    }

    @Test
    fun casesReplayTheFixture() = runBlocking {
        staleTree.getValue("cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val name = case.str("name")
            val server = case.str("server")
            val local = when (case.str("local")) {
                "missing" -> null
                "keyless" -> LocalRoot(root, null)
                "keyed" -> LocalRoot(root, localKey)
                else -> error("unknown local in $name")
            }
            val agent = case.str("send") == "agent"
            val asks = mutableListOf<String>()
            val serverTreeKey = serverKey.takeIf { server == "keyed" }
            val wire = StaleTree.resolve(
                conversation = agent,
                explicitId = if (agent) null else root,
                local = local,
                fresh = false,
                sendTarget = {
                    asks += "sendTarget"
                    ServerTree(root.takeIf { server != "none" }, serverTreeKey)
                },
                publicKey = {
                    asks += "publicKeys"
                    serverTreeKey
                },
            )
            val sent = when (wire) {
                TreeWire.Inline -> "inline"
                is TreeWire.Sealed -> "sealed"
                is TreeWire.Create -> {
                    var created = false
                    val key = StaleTree.keyFirst(
                        wire.rootId,
                        create = {
                            asks += "create"
                            if (server == "keyed") throw TrpcException(STALE_TREE_REFUSAL, code = "CONFLICT")
                            created = true
                            PublicTreeKey("new-key", 1)
                        },
                        publicKey = {
                            asks += "publicKeys"
                            serverTreeKey
                        },
                    )
                    when {
                        key == null -> "plaintext"
                        created -> "create"
                        else -> "sealed"
                    }
                }
                TreeWire.Unknown -> "plaintext"
            }
            assertEquals(name, case.getValue("asks").jsonArray.map { it.jsonPrimitive.content }, asks)
            assertEquals(name, case.str("wire"), sent)
        }
    }

    @Test
    fun retryCasesReplayTheFixture() = runBlocking {
        staleTree.getValue("retryCases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val name = case.str("name")
            val refusals = case.getValue("refusals").jsonPrimitive.int
            val fresh = mutableListOf<Boolean>()
            val outcome = try {
                StaleTree.sendOnce { isFresh ->
                    fresh += isFresh
                    if (fresh.size <= refusals) throw TrpcException(STALE_TREE_REFUSAL, code = "CONFLICT")
                    "sent"
                }
            } catch (err: TrpcException) {
                assertEquals(name, STALE_TREE_REFUSAL, err.message)
                "failed"
            }
            assertEquals(name, case.getValue("sends").jsonPrimitive.int, fresh.size)
            assertEquals(name, listOf(false, true).take(fresh.size), fresh)
            assertEquals(name, case.str("outcome"), outcome)
        }
    }

    @Test
    fun anOlderServersEpochRefusalSurfacesAsTheRefusal() = runBlocking {
        try {
            StaleTree.sendOnce<String> { throw TrpcException("The tree is at epoch 1") }
            fail("expected the refusal")
        } catch (err: TrpcException) {
            assertEquals(STALE_TREE_REFUSAL, err.message)
        }
    }

    @Test
    fun anotherFailureIsNotRetried() = runBlocking {
        var sends = 0
        try {
            StaleTree.sendOnce<String> {
                sends++
                throw TrpcException("Not found")
            }
            fail("expected the failure")
        } catch (err: TrpcException) {
            assertEquals("Not found", err.message)
        }
        assertEquals(1, sends)
    }

    /** A fresh resolve asks the server, never Room: the store's conversation is re-asked. */
    @Test
    fun aFreshResolveAsksTheServer() = runBlocking {
        val asks = mutableListOf<String>()
        val wire = StaleTree.resolve(
            conversation = true,
            explicitId = null,
            local = LocalRoot(root, localKey),
            fresh = true,
            sendTarget = {
                asks += "sendTarget"
                ServerTree(root, serverKey)
            },
            publicKey = {
                asks += "publicKeys"
                null
            },
        )
        assertEquals(listOf("sendTarget"), asks)
        assertEquals(TreeWire.Sealed(root, serverKey), wire)
        val session = StaleTree.resolve(
            conversation = false,
            explicitId = root,
            local = null,
            fresh = true,
            sendTarget = { error("no sendTarget for a session") },
            publicKey = { serverKey },
        )
        assertEquals(TreeWire.Sealed(root, serverKey), session)
    }
}
