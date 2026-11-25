package com.exponential.app.data.sealed

import com.exponential.app.data.api.CreateSessionKeyInput
import com.exponential.app.data.api.DueTreeKey
import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.InstallTrustApi
import com.exponential.app.data.api.PendingTreeKey
import com.exponential.app.data.api.PendingWrap
import com.exponential.app.data.api.RequestKeyResult
import com.exponential.app.data.api.RootClaimApproval
import com.exponential.app.data.api.TreeKeyWrap
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.enet.IdentitySlots
import com.exponential.app.data.push.SecretSealer
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-2123 (install-trust.json `keyMaker`): only a JOINED install makes a
 * chat key. Install B of `user-me` (the account's chain = A's root claim)
 * against an HONEST server (the #EXP-2123 server: a waiting install's key
 * writes are FORBIDDEN with [KEY_MAKER_REFUSAL], `due` names it nothing) and
 * a MALICIOUS one (accepts every key write, names every root due and
 * stranded), through the REAL holder ([TreeKeyHolder] + [InstallTrustKeeper],
 * the full eNet binding) and the writer's keying ([KeylessKeying],
 * [newRootKey], [StaleTree.sendOnce]).
 */
class KeyMakerTest {
    private val fx = TrustFixture()
    private val account = "acct"
    private val me = "install-B"
    private val root = "31234567-89ab-4def-8123-456789abcdef"
    private val secret = ByteArray(32) { (0x40 + it).toByte() }

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

    private fun rootOf(name: String): ServedApproval {
        val subject = fx.served("key" to name)
        return ServedApproval(ApprovalKind.Root, subject.endpointId, subject.endpointId, InstallApproval.sign(ApprovalKind.Root, subject, fx.identity(name), EnetTrustCrypto))
    }

    private fun approvalOf(name: String, by: String): ServedApproval {
        val subject = fx.served("key" to name)
        return ServedApproval(ApprovalKind.Approve, subject.endpointId, fx.endpoint(by), InstallApproval.sign(ApprovalKind.Approve, subject, fx.identity(by), EnetTrustCrypto))
    }

    /**
     * The server double. [approved] = A's approve of B is served
     * (`installs.trust`); [lagging] = an honest server whose waiting list
     * still names B after that (it refuses as if B waited).
     */
    private inner class Server(val honest: Boolean) : HolderApi, InstallTrustApi {
        var approved = false
        var lagging = false
        var dueRows: List<DueTreeKey> = emptyList()
        val creates = mutableListOf<CreateSessionKeyInput>()
        val seals = mutableListOf<String>()
        var requestKeys = 0
        val deviceKey = SealedFfi.generateTreeKey()
        private val rootA = rootOf("A")
        private val approveB by lazy { approvalOf("B", "A") }

        /** What the honest server's `installs.waiting` says of B. */
        private val waits get() = honest && (!approved || lagging)

        private fun refusal() = TrpcException(KEY_MAKER_REFUSAL, HttpStatusCode.Forbidden, code = KEY_MAKER_REFUSAL_CODE)

        override suspend fun trust(accountId: String, installId: String) = TrustAnswer(
            listOf(fx.served("key" to "A"), fx.served("key" to "B", "approvalRequested" to true)),
            if (approved) listOf(rootA, approveB) else listOf(rootA),
        )
        override suspend fun requestApproval(accountId: String, installId: String) {}
        override suspend fun claimRoot(accountId: String, installId: String, signature: String, approvals: List<RootClaimApproval>) {}
        override suspend fun approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) {}
        override suspend fun revoke(accountId: String, installId: String) {}

        override suspend fun mine(installId: String, rootIds: List<String>?) = emptyList<HeldTreeKey>()
        override suspend fun pending(installId: String) = emptyList<PendingTreeKey>()
        override suspend fun wrap(installId: String, wraps: List<PendingWrap>) {}

        override suspend fun due(installId: String): List<DueTreeKey> = when {
            !honest -> listOf(DueTreeKey(root, 1, stranded = true))
            waits -> emptyList()
            else -> dueRows
        }

        override suspend fun create(input: CreateSessionKeyInput) {
            if (waits) throw refusal()
            creates += input
        }

        /** `sessions.send`: [seal] = the new root's inline key. */
        fun send(seal: Boolean): String {
            if (seal && waits) throw refusal()
            if (seal) seals += root
            return root
        }

        /** `sessionKeys.requestKey`: the tree's device keys it. */
        fun requestKey(): RequestKeyResult {
            requestKeys++
            return RequestKeyResult("keyed", 1, deviceKey.publicKey)
        }
    }

    private class Install(val keeper: InstallTrustKeeper, val holder: TreeKeyHolder, val vault: TreeKeyVault)

    private fun install(server: Server): Install {
        val persistence = TrustStorePersistence.Memory()
        persistence.save(account, InstallTrustKeeper.json.encodeToString(TrustStore.serializer(), TrustStore(fx.endpoint("B"), fx.key("B").str("userId"))))
        val keeper = InstallTrustKeeper(server, EnetTrustCrypto, persistence, { fx.identity("B") })
        val vault = TreeKeyVault(MapSlots(), XorSealer)
        val holder = TreeKeyHolder(
            accountId = account,
            installId = me,
            installSecret = fx.installSecret("B"),
            crypto = SealedFfi,
            vault = vault,
            api = server,
            installPublic = fx.key("B").str("x25519Public"),
            identity = fx.identity("B"),
            signer = WrapSignerFfi,
            trust = keeper,
            publish = {},
        )
        return Install(keeper, holder, vault)
    }

    /** The writer's keyless keying over [server] (its `sessionKeys.create` as [SealedWriter] makes it). */
    private fun keying(server: Server, install: Install) = KeylessKeying(
        object : KeylessApi {
            override suspend fun makesKeys() = install.holder.makesKeys()
            override suspend fun create(rootId: String, ownOnly: Boolean): PublicTreeKey {
                val key = SealedFfi.generateTreeKey()
                val wrapped = SealedFfi.wrapTreeKey(fx.key("B").str("x25519Public"), key.secret)
                server.create(CreateSessionKeyInput(rootId, me, 1, key.publicKey, listOf(TreeKeyWrap(me, wrapped, null))))
                return PublicTreeKey(key.publicKey, 1)
            }
            override suspend fun requestKey(rootId: String) = server.requestKey()
            override suspend fun currentKey(rootId: String): PublicTreeKey? = null
            override suspend fun publicKey(rootId: String): PublicTreeKey? = null
        },
        keyWaitMs = 50,
        pollMs = 10,
        retryMs = 1,
    )

    /** The first message of a NEW chat, as [SealedWriter] + [com.exponential.app.data.api.SessionsApi.send] send it. */
    private suspend fun newChat(server: Server, install: Install): Result<String> = runCatching {
        StaleTree.sendOnce { _ -> newRootKey({ install.holder.makesKeys() }) { server.send(seal = true) } }
    }

    private fun servers() = listOf(Server(honest = true), Server(honest = false))

    private fun label(server: Server, what: String) = "${if (server.honest) "honest" else "malicious"}: $what"

    // ── a waiting install, against both servers ───────────────────────────

    @Test
    fun waitingTheFirstMessageOfANewChatFailsAndNothingIsSent() = runBlocking {
        for (server in servers()) {
            val b = install(server)
            val sent = newChat(server, b)
            assertEquals(label(server, "refused"), NO_KEY_REFUSAL, sent.exceptionOrNull()?.message)
            assertTrue(label(server, "no seal"), server.seals.isEmpty())
            assertTrue(label(server, "no create"), server.creates.isEmpty())
        }
    }

    @Test
    fun waitingTheFirstMessageIntoAKeylessChatAsksItsDevicesAndSealsToTheirKey() = runBlocking {
        for (server in servers()) {
            val b = install(server)
            val keyed = keying(server, b).key(root)
            assertEquals(label(server, "the device's key"), Keyless.Arrived(PublicTreeKey(server.deviceKey.publicKey, 1)), keyed)
            assertEquals(label(server, "requestKey asked"), 1, server.requestKeys)
            assertTrue(label(server, "no create"), server.creates.isEmpty())
        }
    }

    @Test
    fun waitingNeverWinsADueRotationPassAfterPass() = runBlocking {
        for (server in servers()) {
            val b = install(server)
            // It holds the current epoch: a rotation it would have won before.
            b.vault.put(account, mapOf(root to mapOf(1 to secret)))
            server.dueRows = listOf(DueTreeKey(root, 1))
            repeat(3) { b.holder.pass() }
            assertEquals(label(server, "rotated"), 0, b.holder.rotateDue())
            assertTrue(label(server, "no create"), server.creates.isEmpty())
            assertEquals(label(server, "still epoch 1"), setOf(1), b.vault.all(account)[root].orEmpty().keys)
        }
    }

    @Test
    fun anUnreconciledStoreIsReconciledFirstAndAnOlderServerCountsAsJoined() = runBlocking {
        val server = Server(honest = true)
        val b = install(server)
        assertEquals(InstallTrustState.Unknown, b.keeper.snapshot(account).state)
        assertFalse(b.holder.makesKeys())
        assertEquals(InstallTrustState.Waiting, b.keeper.snapshot(account).state)
        // A server without `installs.trust` (NOT_FOUND): the rules are off.
        val older = object : InstallTrustApi by server {
            override suspend fun trust(accountId: String, installId: String): TrustAnswer =
                throw TrpcException("No procedure installs.trust", HttpStatusCode.NotFound, code = "NOT_FOUND")
        }
        val persistence = TrustStorePersistence.Memory()
        val keeper = InstallTrustKeeper(older, EnetTrustCrypto, persistence, { fx.identity("B") })
        assertTrue(keeper.makesKeys(account, me))
        assertEquals(InstallTrustState.Unavailable, keeper.snapshot(account).state)
    }

    // ── approved, and an honest server that lags ──────────────────────────

    @Test
    fun onceApprovedItKeysAndRotates() = runBlocking {
        for (server in servers()) {
            val b = install(server)
            b.vault.put(account, mapOf(root to mapOf(1 to secret)))
            server.dueRows = listOf(DueTreeKey(root, 1))
            // While it waits: nothing, and nothing remembered.
            b.holder.pass()
            assertTrue(label(server, "waiting"), server.creates.isEmpty())
            server.approved = true
            b.holder.pass()
            assertTrue(label(server, "joined"), b.keeper.store(account)!!.joined)
            assertEquals(label(server, "rotated to 2"), listOf(2), server.creates.map { it.epoch })
            assertNotNull(label(server, "own wrap signed"), server.creates.single().wraps.single().signature)
            assertTrue(label(server, "holds 2"), b.vault.all(account)[root].orEmpty().containsKey(2))
            // A keyless root: created; a new chat: sealed inline.
            assertTrue(label(server, "created"), keying(server, b).key("41234567-89ab-4def-8123-456789abcdef") is Keyless.Created)
            assertEquals(label(server, "requestKey not asked"), 0, server.requestKeys)
            assertTrue(label(server, "new chat sent"), newChat(server, b).isSuccess)
            assertEquals(label(server, "sealed"), 1, server.seals.size)
        }
    }

    @Test
    fun aJoinedInstallTheHonestServerStillRefusesDoesNotRememberIt() = runBlocking {
        val server = Server(honest = true)
        server.approved = true
        server.lagging = true
        server.dueRows = listOf(DueTreeKey(root, 1))
        val b = install(server)
        b.vault.put(account, mapOf(root to mapOf(1 to secret)))
        // `due` names nothing to a waiting install; a lag that still names it is refused.
        val lagged = object : HolderApi by server {
            override suspend fun due(installId: String) = listOf(DueTreeKey(root, 1))
        }
        val holder = TreeKeyHolder(account, me, fx.installSecret("B"), SealedFfi, b.vault, lagged, fx.key("B").str("x25519Public"), fx.identity("B"), WrapSignerFfi, b.keeper, publish = {})
        assertTrue(holder.makesKeys())
        assertEquals(0, holder.rotateDue())
        assertTrue(server.creates.isEmpty())
        // The keyless root asks its devices; the new chat fails with the no-key refusal, not the raw one.
        assertEquals(Keyless.Arrived(PublicTreeKey(server.deviceKey.publicKey, 1)), keying(server, b).key(root))
        assertEquals(1, server.requestKeys)
        assertEquals(NO_KEY_REFUSAL, newChat(server, b).exceptionOrNull()?.message)
        assertTrue(server.seals.isEmpty())
        // The server catches up: the same (root, epoch) is rotated, it was not left.
        server.lagging = false
        assertEquals(1, holder.rotateDue())
        assertEquals(listOf(2), server.creates.map { it.epoch })
    }

    // ── the repair ────────────────────────────────────────────────────────

    @Test
    fun aJoinedInstallRotatesAStrandedRootItHoldsNoSecretOf() = runBlocking {
        val server = Server(honest = true)
        server.approved = true
        server.dueRows = listOf(DueTreeKey(root, 1, stranded = true))
        val b = install(server)
        assertTrue(b.vault.all(account)[root].isNullOrEmpty())
        b.holder.pass()
        val created = server.creates.single()
        assertEquals(root, created.rootId)
        assertEquals(2, created.epoch)
        assertEquals(listOf(me), created.wraps.map { it.installId })
        assertNotNull(created.wraps.single().signature)
        assertTrue(b.vault.all(account)[root].orEmpty().containsKey(2))
    }

    @Test
    fun aDueRootNotStrandedAndNotHeldStaysAnotherHolders() = runBlocking {
        val server = Server(honest = true)
        server.approved = true
        server.dueRows = listOf(DueTreeKey(root, 1))
        val b = install(server)
        b.holder.pass()
        assertTrue(server.creates.isEmpty())
    }

    @Test
    fun aDueRowWithoutStrandedDecodesAsNotStranded() {
        val json = Json { ignoreUnknownKeys = true }
        assertFalse(json.decodeFromString(DueTreeKey.serializer(), """{"rootId":"r","epoch":1}""").stranded)
        assertTrue(json.decodeFromString(DueTreeKey.serializer(), """{"rootId":"r","epoch":1,"stranded":true}""").stranded)
    }
}
