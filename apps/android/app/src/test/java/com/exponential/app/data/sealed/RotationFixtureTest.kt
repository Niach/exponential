package com.exponential.app.data.sealed

import at.exponential.net.ffi.EnetWrapStatement
import at.exponential.net.ffi.verifyWrapSignature
import com.exponential.app.data.api.CreateSessionKeyInput
import com.exponential.app.data.api.DueTreeKey
import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.PendingTreeKey
import com.exponential.app.data.api.PendingWrap
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.enet.EnetFfi
import com.exponential.app.data.enet.IdentitySlots
import com.exponential.app.data.enet.base64Url
import com.exponential.app.data.push.SecretSealer
import java.io.File
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1951: packages/domain-contract/fixtures/sealed-content.json `rotation`
 * replayed: the pure [rotationEpoch] / [rotationAfter], and `holderCases` =
 * [TreeKeyHolder] passes against a SERVER DOUBLE that answers what each case
 * says, with the real eNet crypto and the fixture's install. The double may
 * keep serving a row wrapped for another install (`serves: foreign`); the
 * holder wraps a new epoch for itself and for the pairs `pending` lists,
 * nobody else.
 */
class RotationFixtureTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.enet.sealedFixture")
            ?: error("exponential.enet.sealedFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }
    private val rotation = fixture.getValue("rotation").jsonObject

    private fun JsonObject.str(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.intOrNull(key: String) = this[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.int
    private fun hex(text: String) = text.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
    private fun JsonObject.epochs(key: String): Map<String, List<Int>> =
        getValue(key).jsonObject.mapValues { (_, list) -> list.jsonArray.map { it.jsonPrimitive.int } }

    private class MapSlots : IdentitySlots {
        val map = mutableMapOf<String, String>()
        override fun get(key: String) = map[key]
        override fun put(key: String, value: String): Boolean {
            map[key] = value
            return true
        }
        override fun remove(key: String) {
            map.remove(key)
        }
    }

    private object XorSealer : SecretSealer {
        override fun seal(plain: ByteArray) = plain.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
        override fun open(blob: ByteArray) = seal(blob)
    }

    @Test
    fun stepCasesReplayTheFixture() {
        val cases = rotation.getValue("stepCases").jsonArray
        assertTrue(cases.size >= 7)
        for (element in cases) {
            val case = element.jsonObject
            val epoch = rotationEpoch(
                flagged = case.intOrNull("flagged"),
                current = case.intOrNull("current"),
                held = case.getValue("held").jsonPrimitive.boolean,
                left = case.getValue("left").jsonPrimitive.boolean,
            )
            assertEquals(case.str("name"), case.intOrNull("epoch"), epoch)
        }
    }

    @Test
    fun answerCasesReplayTheFixture() {
        val cases = rotation.getValue("answerCases").jsonArray
        assertTrue(cases.size >= 7)
        for (element in cases) {
            val case = element.jsonObject
            val failure = case["failure"]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content
                ?.let { name -> CreateFailure.entries.single { it.name.replaceFirstChar(Char::lowercase) == name } }
            assertEquals(case.str("name"), case.str("then"), rotationAfter(failure).name.replaceFirstChar(Char::lowercase))
        }
    }

    private data class Create(val rootId: String, val epoch: Int, val wraps: List<String>)
    private data class Wrap(val rootId: String, val epoch: Int, val to: String)

    private inner class ServerDouble(private val case: JsonObject) : HolderApi {
        private val answers = case.getValue("create").jsonArray.map { it.jsonPrimitive.content }.toMutableList()
        private val readers = case.getValue("readers").jsonArray.map { it.jsonPrimitive.content }
        private val done = mutableSetOf<String>()
        private val conflicted = mutableMapOf<String, Int>()
        private val created = mutableListOf<Triple<String, Int, String>>()
        val creates = mutableListOf<Create>()
        val wraps = mutableListOf<Wrap>()

        override suspend fun mine(installId: String, rootIds: List<String>?): List<HeldTreeKey> {
            val foreign = if (case["serves"]?.jsonPrimitive?.content == "foreign") {
                case.getValue("held").jsonObject.keys.map { HeldTreeKey(it, 9, rotation.str("foreignWrap")) }
            } else {
                emptyList()
            }
            return (foreign + conflicted.map { (rootId, epoch) -> HeldTreeKey(rootId, epoch, rotation.str("rotatedWrap")) })
                .filter { rootIds == null || it.rootId in rootIds }
        }

        override suspend fun due(installId: String) =
            case.getValue("due").jsonArray.map { it.jsonObject }.map { DueTreeKey(it.str("rootId"), it.getValue("epoch").jsonPrimitive.int) }.filter { it.rootId !in done }

        override suspend fun create(input: CreateSessionKeyInput) {
            creates += Create(input.rootId, input.epoch, input.wraps.map { if (it.installId == SELF) "self" else it.installId })
            // Its own wrap really opens with this install's secret.
            assertEquals(32, SealedFfi.unwrapTreeKey(installSecret, input.wraps.single().wrapped).size)
            when (val answer = if (answers.isEmpty()) "ok" else answers.removeAt(0)) {
                "ok" -> {
                    done += input.rootId
                    created += Triple(input.rootId, input.epoch, input.publicKey)
                }
                "blip" -> throw java.io.IOException("no answer")
                "CONFLICT" -> {
                    done += input.rootId
                    conflicted[input.rootId] = input.epoch
                    throw TrpcException("The tree is at epoch ${input.epoch}", code = answer)
                }
                else -> throw TrpcException("refused", code = answer)
            }
        }

        override suspend fun pending(installId: String) = created.flatMap { (rootId, epoch, publicKey) ->
            readers.filter { to -> wraps.none { it.rootId == rootId && it.to == to } }
                .map { PendingTreeKey(rootId, epoch, publicKey, it, fixture.getValue("tree").jsonObject.str("publicKey")) }
        }

        override suspend fun wrap(installId: String, wraps: List<PendingWrap>) {
            this.wraps += wraps.map { Wrap(it.rootId, it.epoch, it.targetInstallId) }
        }
    }

    private val installSecret by lazy { hex(fixture.getValue("install").jsonObject.str("secret")) }

    @Test
    fun theVectorsOpenAsTheFixtureSays() {
        assertArrayEquals(hex(fixture.getValue("rotatedTree").jsonObject.str("secret")), SealedFfi.unwrapTreeKey(installSecret, rotation.str("rotatedWrap")))
        assertTrue(runCatching { SealedFfi.unwrapTreeKey(installSecret, rotation.str("foreignWrap")) }.isFailure)
    }

    @Test
    fun holderCasesReplayTheFixture() = runBlocking {
        val secretAt = mapOf(
            1 to hex(fixture.getValue("tree").jsonObject.str("secret")),
            2 to hex(fixture.getValue("rotatedTree").jsonObject.str("secret")),
        )
        val cases = rotation.getValue("holderCases").jsonArray
        assertTrue(cases.size >= 10)
        for (element in cases) {
            val case = element.jsonObject
            val name = case.str("name")
            val held = case.epochs("held")
            val vault = TreeKeyVault(MapSlots(), XorSealer)
            vault.put("acct", held.mapValues { (_, epochs) -> epochs.associateWith { secretAt.getValue(it) } })
            val server = ServerDouble(case)
            val holder = TreeKeyHolder(
                "acct", SELF, installSecret, SealedFfi, vault, server,
                installPublic = fixture.getValue("install").jsonObject.str("x25519Public"),
            ) {}
            repeat(case.intOrNull("passes") ?: 1) { holder.pass() }

            val creates = case.getValue("creates").jsonArray.map { it.jsonObject }
                .map { Create(it.str("rootId"), it.getValue("epoch").jsonPrimitive.int, it.getValue("wraps").jsonArray.map { w -> w.jsonPrimitive.content }) }
            assertEquals(name, creates, server.creates)
            val wraps = case.getValue("wraps").jsonArray.map { it.jsonObject }.map { Wrap(it.str("rootId"), it.getValue("epoch").jsonPrimitive.int, it.str("to")) }
            assertEquals(name, wraps, server.wraps)
            val holds = case.epochs("holds")
            for (root in held.keys + holds.keys) {
                assertEquals("$name: $root", holds[root].orEmpty(), vault.all("acct")[root].orEmpty().keys.sorted())
            }
        }
    }

    /** #EXP-2119 (OLDER EPOCHS): the held secret of exactly (root, epoch) leaves, signed over its own public key. */
    @Test
    fun olderEpochCasesReplayTheFixture() = runBlocking {
        val root = "00000000-0000-4000-8000-000000000001"
        val tree = fixture.getValue("tree").jsonObject
        val rotated = fixture.getValue("rotatedTree").jsonObject
        val secretAt = mapOf(1 to hex(tree.str("secret")), 2 to hex(rotated.str("secret")))
        val publicAt = mapOf(1 to tree.str("publicKey"), 2 to rotated.str("publicKey"))
        val own = ByteArray(32) { 7 }
        val target = ByteArray(32) { 8 }
        val cases = rotation.getValue("olderEpochCases").jsonArray
        assertTrue(cases.size >= 3)
        for (element in cases) {
            val case = element.jsonObject
            val name = case.str("name")
            val pair = case.getValue("pair").jsonObject
            val epoch = pair.getValue("epoch").jsonPrimitive.int
            val sent = mutableListOf<PendingWrap>()
            val server = object : HolderApi {
                override suspend fun mine(installId: String, rootIds: List<String>?) = emptyList<HeldTreeKey>()
                override suspend fun pending(installId: String) = listOf(
                    PendingTreeKey(
                        root, epoch, if (pair.str("publicKey") == "tree") tree.str("publicKey") else pair.str("publicKey"),
                        "t", base64Url(EnetFfi.x25519PublicOf(target)), WrapSignerFfi.endpointIdOf(target),
                    ),
                )
                override suspend fun wrap(installId: String, wraps: List<PendingWrap>) {
                    sent += wraps
                }
            }
            val vault = TreeKeyVault(MapSlots(), XorSealer)
            vault.put("acct", mapOf(root to case.getValue("held").jsonArray.map { it.jsonPrimitive.int }.associateWith { secretAt.getValue(it) }))
            TreeKeyHolder(
                "acct", SELF, SealedFfi.installSecret(own), SealedFfi, vault, server,
                installPublic = base64Url(EnetFfi.x25519PublicOf(own)), identity = own, signer = WrapSignerFfi,
            ) {}.answerPending()
            val wraps = case.intOrNull("wraps")
            if (wraps == null) {
                assertTrue(name, sent.isEmpty())
                continue
            }
            val wrap = sent.single()
            assertEquals(name, epoch, wrap.epoch)
            assertArrayEquals(name, secretAt.getValue(wraps), SealedFfi.unwrapTreeKey(SealedFfi.installSecret(target), wrap.wrapped))
            val statement = EnetWrapStatement(root, epoch.toUInt(), publicAt.getValue(wraps), WrapSignerFfi.endpointIdOf(target), wrap.wrapped)
            assertTrue(name, verifyWrapSignature(statement, wrap.signature!!, WrapSignerFfi.endpointIdOf(own)))
        }
    }

    private companion object {
        const val SELF = "install-me"
    }
}
