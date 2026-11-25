package com.exponential.app.data.sealed

import at.exponential.net.ffi.EnetWrapStatement
import at.exponential.net.ffi.verifyWrapSignature
import com.exponential.app.data.api.CreateSessionKeyInput
import com.exponential.app.data.api.DueTreeKey
import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.PendingTreeKey
import com.exponential.app.data.api.PendingWrap
import com.exponential.app.data.api.RESIGN_MAX
import com.exponential.app.data.api.ResignResult
import com.exponential.app.data.api.ResignWrap
import com.exponential.app.data.api.TreePublicKey
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.enet.EnetFfi
import com.exponential.app.data.enet.IdentitySlots
import com.exponential.app.data.enet.base64Url
import com.exponential.app.data.push.SecretSealer
import com.exponential.app.domain.contractFixtureJson
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1950: the holder SIGNS every wrap it makes (wrap-signature.json),
 * against a key API that verifies like the server (#3394, `resign` #3408):
 * a signature must be the caller's install's over (root, epoch, the STORED
 * tree public key, the target's endpoint id, the wrapped bytes), else the
 * write is refused.
 */
class TreeKeyHolderSigningTest {
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

    private class Install(val id: String, seed: Int) {
        val identity = ByteArray(32) { (it * 7 + seed).toByte() }
        val endpointId: String = WrapSignerFfi.endpointIdOf(identity)
        val x25519: String = base64Url(EnetFfi.x25519PublicOf(identity))
        val secret: ByteArray = SealedFfi.installSecret(identity)
        val target = WrapTarget(id, endpointId, x25519)
    }

    private data class Row(val root: String, val epoch: Int, val install: String, var wrapped: String, var signature: String?, var signer: String?)

    /** The server's key rules with its signature checks. */
    private class VerifyingApi(val installs: Map<String, Install>, val readers: MutableList<String>) : HolderApi {
        val roots = mutableMapOf<String, Pair<Int, String>>()
        val rows = mutableListOf<Row>()
        val due = mutableListOf<DueTreeKey>()
        val legacy = mutableListOf<String>()
        val signed = mutableListOf<String>()
        val resignCalls = mutableListOf<List<ResignWrap>>()
        var resignCode: String? = null
        var oldPending = false
        /** A tree whose readers are not [readers] (a private chat): its own list. */
        val rootReaders = mutableMapOf<String, List<String>>()

        fun readersOf(root: String): List<String> = rootReaders[root] ?: readers

        fun verifies(caller: String, root: String, epoch: Int, target: String, wrapped: String, signature: String): Boolean {
            val (stored, public) = roots[root] ?: return false
            if (stored != epoch) return false
            return verifyWrapSignature(EnetWrapStatement(root, epoch.toUInt(), public, installs.getValue(target).endpointId, wrapped), signature, installs.getValue(caller).endpointId)
        }

        fun store(caller: String, root: String, epoch: Int, target: String, wrapped: String, signature: String?) {
            if (signature != null) signed += wrapped
            rows += Row(root, epoch, target, wrapped, signature, signature?.let { installs.getValue(caller).endpointId })
        }

        /** Rows from before #EXP-1950: unsigned wraps of [tree] for [targets]. */
        fun legacy(root: String, epoch: Int, tree: TreeKey, targets: List<String>) {
            roots[root] = epoch to tree.publicKey
            for (target in targets) {
                val wrapped = SealedFfi.wrapTreeKey(installs.getValue(target).x25519, tree.secret)
                legacy += wrapped
                store("", root, epoch, target, wrapped, null)
            }
        }

        private fun badSignature() = TrpcException("A wrap's signature is not your install's over this key", code = "BAD_REQUEST")

        override suspend fun mine(installId: String, rootIds: List<String>?) =
            rows.filter { it.install == installId && (rootIds == null || it.root in rootIds) }
                .map { HeldTreeKey(it.root, it.epoch, it.wrapped, it.signature, it.signer) }

        override suspend fun pending(installId: String): List<PendingTreeKey> = roots.flatMap { (root, key) ->
            val (epoch, public) = key
            if (rows.none { it.root == root && it.epoch == epoch && it.install == installId }) return@flatMap emptyList()
            readersOf(root).filter { reader -> rows.none { it.root == root && it.epoch == epoch && it.install == reader } }.map { reader ->
                val install = installs.getValue(reader)
                PendingTreeKey(root, epoch, public, reader, install.x25519, install.endpointId.takeUnless { oldPending })
            }
        }

        override suspend fun wrap(installId: String, wraps: List<PendingWrap>) {
            if (wraps.any { w -> w.signature?.let { !verifies(installId, w.rootId, w.epoch, w.targetInstallId, w.wrapped, it) } == true }) throw badSignature()
            wraps.forEach { store(installId, it.rootId, it.epoch, it.targetInstallId, it.wrapped, it.signature) }
        }

        override suspend fun due(installId: String) = due.toList()

        override suspend fun create(input: CreateSessionKeyInput) {
            val current = roots[input.rootId]
            if (current == null && input.epoch != 1 || current != null && input.epoch != current.first + 1) throw TrpcException("keyed", code = "CONFLICT")
            roots[input.rootId] = input.epoch to input.publicKey
            if (input.wraps.any { w -> w.signature?.let { !verifies(input.installId, input.rootId, input.epoch, w.installId, w.wrapped, it) } == true }) {
                if (current == null) roots.remove(input.rootId) else roots[input.rootId] = current
                throw badSignature()
            }
            due.removeAll { it.rootId == input.rootId }
            input.wraps.forEach { store(input.installId, input.rootId, input.epoch, it.installId, it.wrapped, it.signature) }
        }

        override suspend fun publicKeys(rootIds: List<String>) =
            roots.filterKeys { it in rootIds }.map { (root, key) -> TreePublicKey(root, key.first, key.second) }

        override suspend fun resign(installId: String, wraps: List<ResignWrap>): ResignResult {
            resignCalls += wraps
            resignCode?.let { throw TrpcException("refused", code = it) }
            val replace = mutableListOf<ResignWrap>()
            var skipped = 0
            for (w in wraps) {
                val current = roots[w.rootId]?.first == w.epoch
                val row = rows.firstOrNull { it.root == w.rootId && it.epoch == w.epoch && it.install == w.installId }
                val state = when {
                    row == null -> "none"
                    row.signature != null -> "signed"
                    else -> "unsigned"
                }
                val readers = readersOf(w.rootId)
                val valid = current && w.installId in readers && verifies(installId, w.rootId, w.epoch, w.installId, w.wrapped, w.signature)
                val (rule, outcome) = resignRule(installId in readers, current, w.installId in readers, valid, state)
                when {
                    rule == "badSignature" -> throw badSignature()
                    outcome == "refused" -> throw TrpcException("Not a reader of this tree", code = "FORBIDDEN")
                    outcome == "replaced" -> replace += w
                    else -> skipped++
                }
            }
            for (w in replace) {
                val row = rows.first { it.root == w.rootId && it.epoch == w.epoch && it.install == w.installId }
                row.wrapped = w.wrapped
                row.signature = w.signature
                row.signer = installs.getValue(installId).endpointId
                signed += w.wrapped
            }
            return ResignResult(replace.size, skipped)
        }

        /** Every row of (root, epoch) signed by [signer], checked as its recipient would: the statement it builds itself. */
        fun assertSigned(root: String, epoch: Int, signer: Install): Int {
            val of = rows.filter { it.root == root && it.epoch == epoch }
            for (row in of) {
                val recipient = installs.getValue(row.install)
                val secret = SealedFfi.unwrapTreeKey(recipient.secret, row.wrapped)
                val public = roots.getValue(root).second
                assertEquals("the wrap opens to the stored key", "k", SealedFfi.openBody(SealedFfi.sealBody(public, epoch, "k"), mapOf(epoch to secret)).text)
                assertEquals(signer.endpointId, row.signer)
                val statement = EnetWrapStatement(root, epoch.toUInt(), public, recipient.endpointId, row.wrapped)
                assertTrue("$root/$epoch for ${row.install}", verifyWrapSignature(statement, row.signature!!, signer.endpointId))
                assertFalse(verifyWrapSignature(statement.copy(epoch = (epoch + 1).toUInt()), row.signature!!, signer.endpointId))
            }
            return of.size
        }
    }

    private val phone = Install("phone", 1)
    private val desktop = Install("desktop", 2)
    private val laptop = Install("laptop", 3)
    private val installs = listOf(phone, desktop, laptop).associateBy { it.id }
    private var clock = 1_000L

    private fun holder(api: HolderApi, install: Install = phone, vault: TreeKeyVault = TreeKeyVault(MapSlots(), XorSealer), signs: Boolean = true) = TreeKeyHolder(
        "acct", install.id, install.secret, SealedFfi, vault, api,
        installPublic = install.x25519,
        identity = install.identity.takeIf { signs },
        signer = WrapSignerFfi.takeIf { signs },
        now = { clock },
    ) {}

    @Test
    fun aRotationAndThePendingAnswersAreSigned() = runBlocking {
        val api = VerifyingApi(installs, mutableListOf("phone", "desktop", "laptop"))
        val tree = SealedFfi.generateTreeKey()
        api.legacy(ROOT, 1, tree, listOf("phone", "desktop", "laptop"))
        // The laptop is dropped: the phone holds epoch 1 and rotates.
        api.readers.remove("laptop")
        api.rows.removeAll { it.install == "laptop" }
        api.due += DueTreeKey(ROOT, 1)
        val holder = holder(api)
        assertTrue(holder.pass())
        assertEquals(2, api.roots.getValue(ROOT).first)
        // Its own wrap (create) and the desktop's (pending), both signed and verified.
        assertEquals(2, api.assertSigned(ROOT, 2, phone))
    }

    @Test
    fun anOldServersPendingIsAnsweredUnsigned() = runBlocking {
        val api = VerifyingApi(installs, mutableListOf("phone", "desktop")).apply { oldPending = true }
        api.legacy(ROOT, 1, SealedFfi.generateTreeKey(), listOf("phone"))
        holder(api).pass()
        val desktopRow = api.rows.single { it.install == "desktop" }
        assertNull(desktopRow.signature)
    }

    @Test
    fun heldKeysAreSignedAgainOncePerRootWithFreshWraps() = runBlocking {
        val api = VerifyingApi(installs, mutableListOf("phone", "desktop"))
        val tree = SealedFfi.generateTreeKey()
        api.legacy(ROOT, 1, tree, listOf("phone", "desktop"))
        api.legacy(OTHER_ROOT, 1, SealedFfi.generateTreeKey(), listOf("phone"))
        api.rootReaders[OTHER_ROOT] = listOf("phone")
        val holder = holder(api)
        holder.pass()
        assertEquals(1, api.resignCalls.size)
        assertEquals(2, api.resignCalls.single().size)
        assertTrue(api.rows.filter { it.install == "phone" }.all { it.signature != null })
        assertNull("another reader's row is its own holder's", api.rows.single { it.install == "desktop" }.signature)
        val mine = api.rows.single { it.install == "phone" && it.root == ROOT }
        assertArrayEquals(tree.secret, SealedFfi.unwrapTreeKey(phone.secret, mine.wrapped))
        holder.pass()
        holder.pass()
        assertEquals(1, api.resignCalls.size)
        // Never a wrap it did not make: none of the bytes it was served.
        assertTrue(api.signed.none { it in api.legacy })
    }

    @Test
    fun aNewRootsSealIsSignedOnceTheRootExists() = runBlocking {
        val api = VerifyingApi(installs, mutableListOf("phone", "desktop"))
        val tree = SealedFfi.generateTreeKey()
        // What `sessions.send {seal}` stored: unsigned.
        api.legacy(ROOT, 1, tree, listOf("phone", "desktop"))
        val vault = TreeKeyVault(MapSlots(), XorSealer)
        vault.put("acct", mapOf(ROOT to mapOf(1 to tree.secret)))
        val holder = holder(api, vault = vault)
        holder.queueResign(ROOT, 1, tree.publicKey, listOf(phone.target, desktop.target))
        assertEquals(2, holder.resignDue())
        assertEquals(2, api.assertSigned(ROOT, 1, phone))
        assertTrue(api.signed.none { it in api.legacy })
        holder.pass()
        assertEquals(1, api.resignCalls.size)
    }

    @Test
    fun aServerWithoutResignIsAskedAgainOnlyMuchLater() = runBlocking {
        val api = VerifyingApi(installs, mutableListOf("phone")).apply { resignCode = "NOT_FOUND" }
        api.legacy(ROOT, 1, SealedFfi.generateTreeKey(), listOf("phone"))
        val holder = holder(api)
        assertTrue(holder.pass())
        assertTrue(holder.pass())
        assertEquals(1, api.resignCalls.size)
        clock += 31 * 60 * 1000L
        holder.pass()
        assertEquals(2, api.resignCalls.size)
    }

    @Test
    fun aRefusedNewRootFallsBackToItsOwnRow() = runBlocking {
        val api = VerifyingApi(installs, mutableListOf("phone", "desktop"))
        api.legacy(ROOT, 1, SealedFfi.generateTreeKey(), listOf("phone"))
        api.rootReaders[ROOT] = listOf("phone")
        api.rootReaders[THIRD_ROOT] = listOf("phone")
        val holder = holder(api)
        holder.pass()
        assertEquals(1, api.resignCalls.size)
        // A new root's seal named the laptop, no reader any more: refused (FORBIDDEN).
        val tree = SealedFfi.generateTreeKey()
        api.legacy(THIRD_ROOT, 1, tree, listOf("phone"))
        holder.queueResign(THIRD_ROOT, 1, tree.publicKey, listOf(phone.target, laptop.target))
        holder.pass()
        assertEquals(2, api.resignCalls.size)
        assertNull(api.rows.single { it.root == THIRD_ROOT }.signature)
        // Its own row alone next, then nothing more.
        holder.pass()
        assertEquals(3, api.resignCalls.size)
        assertEquals(1, api.assertSigned(THIRD_ROOT, 1, phone))
        holder.pass()
        assertEquals(3, api.resignCalls.size)
    }

    @Test
    fun aHolderWithoutAnIdentitySignsNothing() = runBlocking {
        val api = VerifyingApi(installs, mutableListOf("phone", "desktop"))
        api.legacy(ROOT, 1, SealedFfi.generateTreeKey(), listOf("phone"))
        holder(api, signs = false).pass()
        assertTrue(api.rows.all { it.signature == null })
        assertTrue(api.resignCalls.isEmpty())
    }

    private val fixture by lazy { Json.parseToJsonElement(contractFixtureJson("wrap-signature.json")).jsonObject.getValue("resign").jsonObject }

    @Test
    fun resignCasesReplay() {
        for (case in fixture.getValue("cases").jsonArray.map { it.jsonObject }) {
            fun b(key: String) = case.getValue(key).jsonPrimitive.boolean
            val (rule, outcome) = resignRule(b("callerReader"), b("currentEpoch"), b("targetReader"), b("signatureValid"), case.getValue("row").jsonPrimitive.content)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("rule").jsonPrimitive.content to case.getValue("outcome").jsonPrimitive.content, rule to outcome)
        }
        assertEquals(RESIGN_MAX, fixture.getValue("maxWraps").jsonPrimitive.int)
    }

    @Test
    fun resignFailureCasesReplay() {
        for (case in fixture.getValue("failureCases").jsonArray.map { it.jsonObject }) {
            val want = when (case.getValue("after").jsonPrimitive.content) {
                "notYet" -> ResignAfter.NotYet
                "refused" -> ResignAfter.Refused
                else -> ResignAfter.Retry
            }
            assertEquals(case.getValue("name").jsonPrimitive.content, want, resignAfter(case.getValue("code").jsonPrimitive.contentOrNull))
        }
    }

    private companion object {
        const val ROOT = "01234567-89ab-4def-8123-456789abcdef"
        const val OTHER_ROOT = "11111111-1111-4111-8111-111111111111"
        const val THIRD_ROOT = "22222222-2222-4222-8222-222222222222"

        /** `resignOutcome` (wrap-signature.ts): the first rule of `resign.order` that applies. */
        fun resignRule(callerReader: Boolean, currentEpoch: Boolean, targetReader: Boolean, signatureValid: Boolean, row: String): Pair<String, String> {
            val rule = when {
                !callerReader -> "callerNotReader"
                !currentEpoch -> "staleEpoch"
                !targetReader -> "targetNotReader"
                !signatureValid -> "badSignature"
                row == "none" -> "noRow"
                row == "signed" -> "signedRow"
                else -> "unsignedRow"
            }
            val outcome = when (rule) {
                "callerNotReader", "targetNotReader", "badSignature" -> "refused"
                "unsignedRow" -> "replaced"
                else -> "skipped"
            }
            return rule to outcome
        }
    }
}
