package com.exponential.app.data.sealed

import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.InstallTrustApi
import com.exponential.app.data.api.PendingTreeKey
import com.exponential.app.data.api.PendingWrap
import com.exponential.app.data.api.RootClaimApproval
import com.exponential.app.data.api.SessionSendTarget
import com.exponential.app.data.api.TreePublicKey
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.enet.IdentitySlots
import com.exponential.app.data.enet.hexBytes
import com.exponential.app.data.push.SecretSealer
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1950 THE MALICIOUS SERVER (install-trust.json `maliciousServer`, the
 * threat model's four attacks): ONE fake API ([LyingServer]) that lies in
 * `sessionKeys.mine`, `pending`, `publicKeys`, `sessions.sendTarget` and
 * `installs.trust`, answering EXACTLY each case's `answers` (decoded through
 * the app's own wire types), against the REAL holder ([TreeKeyHolder] +
 * [InstallTrustKeeper], the full eNet binding) of the fixture's victim
 * install A. Every case runs in `record` mode (release 1: its `now`) and
 * `mandatory` mode (release 2: its `mandatory`). Then decision (e) against an
 * honest account server, and the negatives: never a wrap for an install that
 * is not pinned, never an approval by itself, never a row whose code the
 * server supplied.
 */
class InstallTrustAttackTest {
    private val fx = TrustFixture()
    private val account = "acct"
    private val wire = Json { ignoreUnknownKeys = true }
    private val real = ByteArray(32) { (0x20 + it).toByte() }
    private val root0 = "01234567-89ab-4def-8123-456789abcdef"
    private val root2 = "21234567-89ab-4def-8123-456789abcdef"

    /** The public half of [real] (the fixture's honest key of root 0123…, as attack 3's `pending` names it). */
    private val realPublic: String = fx.f.cases("maliciousServer").first { it["attack"]!!.jsonPrimitive.int == 3 }
        .cases("cases").first().obj("answers")["sessionKeys.pending"]!!.jsonArray.first().jsonObject.str("publicKey")

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

    /** The lying server: every procedure the holder asks answers what the case says; everything the holder writes is recorded. */
    private class LyingServer : HolderApi, InstallTrustApi {
        var mineRows: List<HeldTreeKey> = emptyList()
        var pendingRows: List<PendingTreeKey> = emptyList()
        var publicKeyRows: List<TreePublicKey> = emptyList()
        var target: SessionSendTarget = SessionSendTarget()
        var trustAnswer = TrustAnswer()
        val wrapped = mutableListOf<PendingWrap>()
        val approved = mutableListOf<Pair<ApprovalKind, String>>()
        val revoked = mutableListOf<String>()
        var requested = 0
        var claimed = 0

        override suspend fun mine(installId: String, rootIds: List<String>?) = mineRows.filter { rootIds == null || it.rootId in rootIds }
        override suspend fun pending(installId: String) = pendingRows
        override suspend fun wrap(installId: String, wraps: List<PendingWrap>) {
            wrapped += wraps
        }
        override suspend fun publicKeys(rootIds: List<String>) = publicKeyRows.filter { it.rootId in rootIds }

        /** `sessions.sendTarget` (the writer and the call ask it; the holder never does). */
        fun sendTarget(): SessionSendTarget = target

        override suspend fun trust(accountId: String, installId: String) = trustAnswer
        override suspend fun requestApproval(accountId: String, installId: String) {
            requested++
        }
        override suspend fun claimRoot(accountId: String, installId: String, signature: String, approvals: List<RootClaimApproval>) {
            claimed++
        }
        override suspend fun approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) {
            approved += kind to subjectInstallId
        }
        override suspend fun revoke(accountId: String, installId: String) {
            revoked += installId
        }
    }

    /** An account's installs as the server stores them, honest: `claimRoot`
     *  stores the root and its approvals in one go (CONFLICT while another
     *  install is on a chain), `approve` stores one. */
    private class AccountServer(installs: List<ServedInstall>, private val claimGate: Int = 0) : InstallTrustApi {
        val installs = installs.toMutableList()
        val approvals = mutableListOf<ServedApproval>()
        val asked = mutableListOf<String>()
        val claims = mutableListOf<String>()
        private var arrived = 0
        private val gate = CompletableDeferred<Unit>()

        private fun endpoint(installId: String) = installs.first { it.installId == installId }.endpointId

        override suspend fun trust(accountId: String, installId: String) = TrustAnswer(
            installs.map { it.copy(approvalRequested = it.approvalRequested || it.installId in asked) },
            approvals.toList(),
        )

        override suspend fun requestApproval(accountId: String, installId: String) {
            asked += installId
        }

        override suspend fun claimRoot(accountId: String, installId: String, signature: String, approvals: List<RootClaimApproval>) {
            claims += installId
            if (claimGate > 0) {
                if (++arrived >= claimGate) gate.complete(Unit)
                gate.await()
            }
            val me = endpoint(installId)
            val approves = approvals.map { ServedApproval(ApprovalKind.Approve, endpoint(it.subjectInstallId), me, it.signature) }
            if (this.approvals.any { (it.kind == ApprovalKind.Root || it.kind == ApprovalKind.Approve) && it.subjectEndpointId != me }) {
                throw TrpcException("CONFLICT", HttpStatusCode.Conflict, code = "CONFLICT")
            }
            this.approvals += ServedApproval(ApprovalKind.Root, me, me, signature)
            this.approvals += approves
        }

        override suspend fun approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) {
            approvals += ServedApproval(kind, endpoint(subjectInstallId), endpoint(installId), signature)
        }

        override suspend fun revoke(accountId: String, installId: String) {}
    }

    private class Victim(val server: LyingServer, val keeper: InstallTrustKeeper, val holder: TreeKeyHolder, val vault: TreeKeyVault)

    private fun rootOf(name: String): ServedApproval {
        val subject = fx.served("key" to name)
        return ServedApproval(ApprovalKind.Root, subject.endpointId, subject.endpointId, InstallApproval.sign(ApprovalKind.Root, subject, fx.identity(name), EnetTrustCrypto))
    }

    private fun approvalOf(name: String, by: String): ServedApproval {
        val subject = fx.served("key" to name)
        return ServedApproval(ApprovalKind.Approve, subject.endpointId, fx.endpoint(by), InstallApproval.sign(ApprovalKind.Approve, subject, fx.identity(by), EnetTrustCrypto))
    }

    private fun stripped(name: String) = fx.served("key" to name, "registration" to "none")

    private fun storeJson(store: TrustStore) = InstallTrustKeeper.json.encodeToString(TrustStore.serializer(), store)

    private fun holderFor(name: String, installId: String, api: HolderApi, keeper: InstallTrustKeeper, vault: TreeKeyVault = TreeKeyVault(MapSlots(), XorSealer)) =
        TreeKeyHolder(
            accountId = account,
            installId = installId,
            installSecret = fx.installSecret(name),
            crypto = SealedFfi,
            vault = vault,
            api = api,
            identity = fx.identity(name),
            signer = WrapSignerFfi,
            publish = {},
            trust = keeper,
        )

    /**
     * The fixture's victim A (joined; pins A, B, T1; verified epoch 1 of 0123…
     * and 2 of 2123…), its vault holding the real secrets, behind a server
     * whose `installs.trust` serves A, B (approved by A) and T1 (a
     * team-mate's root) truthfully, plus whatever the case adds.
     */
    private fun victim(mode: TrustMode, extra: List<ServedInstall> = emptyList(), roots: List<String> = emptyList()): Victim {
        val server = LyingServer()
        val approvals = mutableListOf(rootOf("A"), approvalOf("B", "A"), rootOf("T1"))
        for (name in roots) approvals += rootOf(name)
        server.trustAnswer = TrustAnswer(listOf(fx.served("key" to "A"), fx.served("key" to "B"), fx.served("key" to "T1")) + extra, approvals)
        val persistence = TrustStorePersistence.Memory()
        persistence.save(account, storeJson(fx.namedStore("victim").store))
        val identity = fx.identity("A")
        val keeper = InstallTrustKeeper(server, EnetTrustCrypto, persistence, { identity }, mode)
        val vault = TreeKeyVault(MapSlots(), XorSealer)
        vault.put(account, mapOf(root0 to mapOf(1 to real), root2 to mapOf(1 to real)))
        return Victim(server, keeper, holderFor("A", "install-A", server, keeper, vault), vault)
    }

    private fun attackCases(): List<Pair<Int, JsonObject>> = fx.f.cases("maliciousServer").flatMap { entry ->
        entry.cases("cases").map { entry["attack"]!!.jsonPrimitive.int to it }
    }

    private fun source(c: JsonObject): JsonObject = fx.f.obj("rules").cases(c.str("replay")).first { it.str("name") == c.str("name") }

    private fun want(c: JsonObject, mode: TrustMode) = c.obj(if (mode == TrustMode.Record) "now" else "mandatory")

    /** A held secret a seal / call case starts from, and its record. */
    private fun seedHeld(v: Victim, source: JsonObject, rootId: String, epoch: Int) {
        val held = source["held"]?.takeIf { it !is JsonNull }?.jsonObject ?: return
        v.vault.put(account, mapOf(rootId to mapOf(epoch to held.str("secret").hexBytes())))
        v.keeper.record(account, rootId, epoch, KeyRecord.of(held.str("record"))!!)
    }

    private fun <T> decode(serializer: kotlinx.serialization.KSerializer<T>, value: JsonElement?): T = wire.decodeFromJsonElement(serializer, value!!)

    // ── the four attacks, every case, now and once mandatory ──────────────

    @Test
    fun everyAttackAgainstTheRealHolder() = runBlocking {
        val ran = mutableMapOf<Int, Int>()
        for ((attack, c) in attackCases()) {
            val name = c.str("name")
            val answers = c.obj("answers")
            val src = source(c)
            for (mode in TrustMode.entries) {
                val label = "$name ($mode)"
                val expected = want(c, mode)
                when (val rule = c.str("replay")) {
                    "keepCases" -> {
                        // Attack 1: `sessionKeys.mine` plants a wrap of a key the server made.
                        val v = victim(mode)
                        val planted = decode(ListSerializer(HeldTreeKey.serializer()), answers["sessionKeys.mine"])
                        v.server.mineRows = planted
                        v.holder.pass()
                        val wrap = planted.first()
                        val kept = v.vault.all(account)[wrap.rootId]?.containsKey(wrap.epoch) == true
                        assertEquals(label, expected["keep"]!!.jsonPrimitive.boolean, kept)
                        if (kept) {
                            assertEquals(label, expected.str("record"), v.keeper.recordOf(account, wrap.rootId, wrap.epoch).wire)
                        } else {
                            assertEquals(label, "refused:${expected.str("reason")}", v.keeper.decisions.last { it.rule == TrustDecision.Rule.R1 }.verdict)
                        }
                    }
                    "sealCases" -> {
                        // Attacks 1 + 2: `sendTarget` / `publicKeys` name a false (root, epoch, key).
                        val v = victim(mode)
                        val (rootId, key) = if (answers["sessions.sendTarget"] != null) {
                            v.server.target = decode(SessionSendTarget.serializer(), answers["sessions.sendTarget"])
                            val served = v.server.sendTarget()
                            served.rootId!! to PublicTreeKey(served.publicKey!!, served.epoch!!, served.vouch)
                        } else {
                            v.server.publicKeyRows = decode(ListSerializer(TreePublicKey.serializer()), answers["sessionKeys.publicKeys"])
                            val rootId = v.server.publicKeyRows.first().rootId
                            val served = v.server.publicKeys(listOf(rootId)).first()
                            rootId to PublicTreeKey(served.publicKey, served.epoch, served.vouch)
                        }
                        seedHeld(v, src, rootId, key.epoch)
                        val sealed = v.holder.sealAllowed(rootId, key.publicKey, key.epoch, key.vouch)
                        assertEquals(label, expected["seal"]!!.jsonPrimitive.boolean, sealed)
                        val verdict = v.keeper.decisions.last { it.rule == TrustDecision.Rule.R3 }.verdict
                        assertEquals(label, if (sealed) expected.str("record") else "refused:${expected.str("reason")}", verdict)
                    }
                    "targetCases" -> {
                        // Attack 3: `pending` + the install list name the server's reader.
                        val listed = decode(ListSerializer(ServedInstall.serializer()), answers["installs.list"])
                        val roots = src["roots"]!!.jsonArray.map { it.jsonPrimitive.content }
                        // B is served truthfully by `installs.trust` (only `pending` lies about its key).
                        val v = victim(mode, extra = listed.filter { it.installId != "install-B" }, roots = roots)
                        val pairs = decode(ListSerializer(PendingTreeKey.serializer()), answers["sessionKeys.pending"])
                        v.server.pendingRows = pairs
                        v.holder.pass()
                        val target = pairs.first().targetInstallId
                        val didWrap = v.server.wrapped.any { it.targetInstallId == target }
                        assertEquals(label, expected["wrap"]!!.jsonPrimitive.boolean, didWrap)
                        val verdict = v.keeper.decisions.last { it.rule == TrustDecision.Rule.R4 && it.subject == target }.verdict
                        assertEquals(label, if (didWrap) expected.str("record") else "refused:${expected.str("reason")}", verdict)
                    }
                    "callCases" -> {
                        // Attack 4: a false `sendTarget` puts the server into a sealed call.
                        val v = victim(mode)
                        v.server.target = decode(SessionSendTarget.serializer(), answers["sessions.sendTarget"])
                        val served = v.server.sendTarget()
                        val rootId = served.rootId!!
                        val epoch = served.epoch!!
                        seedHeld(v, src, rootId, epoch)
                        val secret = v.holder.callSecret(rootId, epoch)
                        assertEquals(label, expected["key"]!!.jsonPrimitive.boolean, secret != null)
                        (expected["noKey"] as? JsonPrimitive)?.content?.let { noKey ->
                            assertEquals(label, noKey, v.keeper.decisions.last { it.rule == TrustDecision.Rule.Call }.verdict)
                        }
                    }
                    else -> throw AssertionError("$name: unknown rule $rule")
                }
            }
            ran[attack] = (ran[attack] ?: 0) + 1
        }
        assertEquals(listOf(1, 2, 3, 4), ran.keys.sorted())
        assertEquals(attackCases().size, ran.values.sum())
        println("install-trust attacks: ${ran.values.sum()} cases × 2 modes against the real holder (${ran.toSortedMap()})")
    }

    // ── the negatives ─────────────────────────────────────────────────────

    /** A rotation, a resign and a writer's readers never include an install
     *  nobody pinned (one that asked, or a silent own one), whatever the server lists. */
    @Test
    fun neverWrapsForAnUnpinnedInstall() = runBlocking {
        val asking = fx.served("key" to "Z", "approvalRequested" to true)
        val v = victim(TrustMode.Record, extra = listOf(asking))
        v.holder.pass()
        val a = WrapTarget("install-A", fx.endpoint("A"), fx.key("A").str("x25519Public"))
        val b = WrapTarget("install-B", fx.endpoint("B"), fx.key("B").str("x25519Public"))
        val z = WrapTarget("install-Z", asking.endpointId, asking.x25519Public)
        assertEquals(listOf("install-A", "install-B"), v.holder.allowed(listOf(a to "user-me", b to "user-me", z to "user-me")).map { it.installId })
        // A silent own install the server lists (decision e): refused too.
        val silent = fx.served("key" to "N")
        val w = victim(TrustMode.Record, extra = listOf(silent))
        w.holder.pass()
        val n = WrapTarget("install-N", silent.endpointId, silent.x25519Public)
        assertEquals(listOf("install-A"), w.holder.allowed(listOf(a to "user-me", n to "user-me")).map { it.installId })
        // Its own new key is recorded verified (it made it).
        w.holder.recordMade("41234567-89ab-4def-8123-456789abcdef", 1)
        assertEquals(KeyRecord.Verified, w.keeper.recordOf(account, "41234567-89ab-4def-8123-456789abcdef", 1))
    }

    /** A waiting install wraps for nobody but itself. */
    @Test
    fun aWaitingInstallWrapsForNobodyElse() = runBlocking {
        val server = LyingServer()
        val a = fx.served("key" to "A")
        val n = fx.served("key" to "N", "approvalRequested" to true)
        server.trustAnswer = TrustAnswer(listOf(a, n), listOf(rootOf("A")))
        val identity = fx.identity("N")
        val keeper = InstallTrustKeeper(server, EnetTrustCrypto, TrustStorePersistence.Memory(), { identity })
        val vault = TreeKeyVault(MapSlots(), XorSealer)
        vault.put(account, mapOf(root0 to mapOf(1 to real)))
        val holder = holderFor("N", "install-N", server, keeper, vault)
        server.pendingRows = listOf(PendingTreeKey(root0, 1, realPublic, "install-A", a.x25519Public, a.endpointId))
        holder.pass()
        val snapshot = keeper.snapshot(account)
        assertEquals(InstallTrustState.Waiting, snapshot.state)
        assertEquals(fx.key("N").str("code"), snapshot.code)
        assertTrue(snapshot.rows.isEmpty())
        assertTrue(server.wrapped.isEmpty())
        // It had asked already (`approvalRequested`): never again; never claimed a root.
        assertEquals(0, server.requested)
        assertEquals(0, server.claimed)
        assertEquals("refused:notTrusted", keeper.decisions.last { it.rule == TrustDecision.Rule.R4 }.verdict)
    }

    /** A trusted install shows the asking install as a row and NEVER approves
     *  it by itself, over any number of passes; Approve signs and posts;
     *  Remove revokes; a Reset's Remove declines. */
    @Test
    fun neverApprovesByItselfAndApprovesOnlyWhenPressed() = runBlocking {
        val asking = fx.served("key" to "N", "approvalRequested" to true)
        val t3 = fx.served("key" to "T3")
        val v = victim(TrustMode.Record, extra = listOf(asking, t3), roots = listOf("T3"))
        repeat(3) { v.holder.pass() }
        assertTrue(v.server.approved.isEmpty())
        assertEquals(0, v.server.requested)
        val snapshot = v.keeper.snapshot(account)
        assertEquals(InstallTrustState.Trusted, snapshot.state)
        assertEquals(listOf(TrustRow.Kind.NewInstall, TrustRow.Kind.Reset), snapshot.rows.map { it.kind })
        assertEquals(listOf("install-N", "install-T3"), snapshot.rows.map { it.installId })
        // The UI's flow carries the same rows.
        assertEquals(snapshot, v.keeper.snapshots.value[account])
        v.keeper.approve(account, snapshot.rows[0])
        assertEquals(listOf(ApprovalKind.Approve to "install-N"), v.server.approved)
        v.keeper.remove(account, snapshot.rows[0])
        assertEquals(listOf("install-N"), v.server.revoked)
        v.keeper.remove(account, snapshot.rows[1])
        assertEquals(listOf(ApprovalKind.Approve, ApprovalKind.Decline), v.server.approved.map { it.first })
    }

    /** The code of a row is derived from its endpoint id HERE: a server that
     *  sends its own `code` never puts it on a row, and a row whose
     *  registration does not verify is never shown. */
    @Test
    fun neverShowsARowWhoseCodeTheServerSupplied() = runBlocking {
        val lyingJson = JsonObject(
            Json.encodeToJsonElement(ServedInstall.serializer(), fx.served("key" to "N", "approvalRequested" to true)).jsonObject +
                ("code" to JsonPrimitive("AAAAA AAAAA")),
        )
        val lying = wire.decodeFromJsonElement(ServedInstall.serializer(), lyingJson)
        val forged = fx.served("key" to "L", "approvalRequested" to true, "registration" to "forged")
        val v = victim(TrustMode.Record, extra = listOf(lying, forged))
        v.holder.pass()
        val rows = v.keeper.snapshot(account).rows
        assertEquals(listOf("install-N"), rows.map { it.installId })
        assertEquals(fx.key("N").str("code"), rows.first().code)
        assertEquals(InstallCode.of(lying.endpointId), rows.first().code)
        assertTrue(v.server.approved.isEmpty())
    }

    /** An older server without `installs.trust`: everything as before (unavailable, legacy). */
    @Test
    fun anOlderServerChangesNothing() = runBlocking {
        val old = object : InstallTrustApi {
            override suspend fun trust(accountId: String, installId: String): TrustAnswer =
                throw TrpcException("No procedure installs.trust", HttpStatusCode.NotFound, code = "NOT_FOUND")
            override suspend fun requestApproval(accountId: String, installId: String) {}
            override suspend fun claimRoot(accountId: String, installId: String, signature: String, approvals: List<RootClaimApproval>) {}
            override suspend fun approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) {}
            override suspend fun revoke(accountId: String, installId: String) {}
        }
        val identity = fx.identity("A")
        val keeper = InstallTrustKeeper(old, EnetTrustCrypto, TrustStorePersistence.Memory(), { identity })
        keeper.sync(account, "install-A")
        assertEquals(InstallTrustState.Unavailable, keeper.snapshot(account).state)
        assertEquals(
            InstallTrust.TargetVerdict.Wrap(KeyRecord.Legacy),
            keeper.target(account, "install-Z", fx.key("Z").str("x25519Public"), null, null),
        )
    }

    /** Attack 3's gap, now only under a TEAM-MATE's account: a silent install
     *  there looks like an older build: wrapped and recorded `legacy` now,
     *  refused once mandatory. */
    @Test
    fun aSilentTeamMatesInstallIsTheLegacyGap() = runBlocking {
        val t2 = fx.served("key" to "T2")
        for (mode in TrustMode.entries) {
            val v = victim(mode, extra = listOf(t2))
            v.server.pendingRows = listOf(PendingTreeKey(root0, 1, realPublic, "install-T2", t2.x25519Public, t2.endpointId))
            v.holder.pass()
            assertEquals("$mode", if (mode == TrustMode.Record) listOf("install-T2") else emptyList<String>(), v.server.wrapped.map { it.targetInstallId })
            val decided = v.keeper.decisions.last { it.rule == TrustDecision.Rule.R4 && it.subject == "install-T2" }
            assertEquals("$mode", if (mode == TrustMode.Record) "legacy" else "refused:unapproved", decided.verdict)
        }
    }

    // ── decision (e): the installs that existed are trusted once ──────────

    /** An install that EXISTED: its identity is there, its pin store is not yet. */
    private fun existingInstall(name: String, api: InstallTrustApi, mode: TrustMode = TrustMode.Record): InstallTrustKeeper {
        val identity = fx.identity(name)
        val keeper = InstallTrustKeeper(api, EnetTrustCrypto, TrustStorePersistence.Memory(), { identity }, mode)
        assertEquals(name, true, keeper.store(account)?.existing)
        return keeper
    }

    @Test
    fun dayOneNoRowNoWaitingTheFirstClaimsWithEveryApprovalTheSecondJoinsOnItsChain() = runBlocking {
        val server = AccountServer(listOf(fx.served("key" to "A"), fx.served("key" to "B"), stripped("L")))
        val a = existingInstall("A", server)
        a.sync(account, "install-A")
        assertEquals(InstallTrustState.Trusted, a.snapshot(account).state)
        assertEquals(emptyList<TrustRow>(), a.snapshot(account).rows)
        assertEquals(listOf("approve:B", "approve:L", "root:A"), server.approvals.map { "${it.kind.wire}:${fx.nameOf(it.subjectEndpointId)}" }.sorted())
        val b = existingInstall("B", server)
        b.sync(account, "install-B")
        assertEquals(InstallTrustState.Trusted, b.snapshot(account).state)
        assertEquals(emptyList<TrustRow>(), b.snapshot(account).rows)
        val l = b.store(account)!!.pins[fx.endpoint("L")]
        assertEquals(PinVia.Approved, l?.via)
        assertEquals(fx.endpoint("A"), l?.by)
        assertEquals(emptyList<String>(), server.asked)
        assertEquals(listOf("install-A"), server.claims)
    }

    @Test
    fun twoExistingInstallsAtOnceTheSecondConflictsAndNeverWaits() = runBlocking {
        // Both read "no chain" before either claims.
        val server = AccountServer(listOf(fx.served("key" to "A"), fx.served("key" to "B")), claimGate = 2)
        val a = existingInstall("A", server)
        val b = existingInstall("B", server)
        listOf(async { a.sync(account, "install-A") }, async { b.sync(account, "install-B") }).awaitAll()
        for (keeper in listOf(a, b)) {
            assertEquals(InstallTrustState.Trusted, keeper.snapshot(account).state)
            assertEquals(emptyList<TrustRow>(), keeper.snapshot(account).rows)
        }
        assertEquals(2, server.claims.size)
        // A waiting state would have asked.
        assertEquals(emptyList<String>(), server.asked)
        assertEquals(1, server.approvals.count { it.kind == ApprovalKind.Root })
    }

    @Test
    fun aZPlantedAfterTheFirstReconcileIsARowNeverWrappedNeverApproved() = runBlocking {
        for (mode in TrustMode.entries) {
            val server = AccountServer(listOf(fx.served("key" to "A"), fx.served("key" to "B"), stripped("L")))
            val a = existingInstall("A", server, mode)
            a.sync(account, "install-A")
            assertEquals("$mode", false, a.store(account)?.existing)
            // The lie: Z silent (never asked, no root), as an install from before the release would be.
            val z = fx.served("key" to "Z")
            server.installs += z
            a.sync(account, "install-A")
            assertNull("$mode", a.store(account)!!.pins[z.endpointId])
            assertEquals("$mode", listOf("newInstall:install-Z"), a.snapshot(account).rows.map { "${it.kind.wire}:${it.installId}" })
            val keys = LyingServer()
            keys.pendingRows = listOf(PendingTreeKey(root0, 1, realPublic, "install-Z", z.x25519Public, z.endpointId))
            val vault = TreeKeyVault(MapSlots(), XorSealer)
            vault.put(account, mapOf(root0 to mapOf(1 to real)))
            // The holder's pass follows the SAME server first (its trust step), then answers `pending`.
            val holder = holderFor("A", "install-A", keys, a, vault)
            holder.pass()
            assertEquals("$mode", emptyList<String>(), keys.wrapped.map { it.targetInstallId })
            assertEquals("$mode", "refused:unapproved", a.decisions.last { it.rule == TrustDecision.Rule.R4 && it.subject == "install-Z" }.verdict)
            // And it never approved Z by itself.
            assertEquals("$mode", 0, server.approvals.count { it.subjectEndpointId == z.endpointId })
        }
    }

    @Test
    fun aHiddenChainAfterTheFirstReconcileWaits() = runBlocking {
        val server = AccountServer(listOf(fx.served("key" to "A"), fx.served("key" to "B")))
        server.approvals += rootOf("A")
        val b = existingInstall("B", server)
        b.sync(account, "install-B")
        // Existing on this install, but not on its account's chain: new.
        assertEquals(InstallTrustState.Waiting, b.snapshot(account).state)
        assertEquals(false, b.store(account)?.existing)
        server.approvals.clear()
        server.installs += fx.served("key" to "Z")
        b.sync(account, "install-B")
        assertEquals(InstallTrustState.Waiting, b.snapshot(account).state)
        assertEquals(listOf(fx.endpoint("B")), b.store(account)!!.pins.keys.sorted())
        assertEquals(emptyList<String>(), server.claims)
    }

    /** A new identity (Android mints one per account at sign-in): its store
     *  is written BEFORE it registers ([InstallTrustKeeper.markNew], through
     *  the identity vault), so it is a new install: it waits and asks, it adopts nobody. */
    @Test
    fun aNewIdentityWritesItsStoreFirstAndWaits() = runBlocking {
        val identity = fx.identity("N")
        val persistence = TrustStorePersistence.Memory()
        // The identity vault mints it and calls back before anything registers.
        val minted = com.exponential.app.data.enet.EnetIdentityVault(MapSlots(), XorSealer, generate = { identity }) { accountId, id ->
            InstallTrustKeeper.markNew(accountId, id, EnetTrustCrypto, persistence)
        }
        assertArrayEquals(identity, minted.identity(account))
        val server = AccountServer(listOf(fx.served("key" to "A"), fx.served("key" to "B"), fx.served("key" to "N")))
        server.approvals += listOf(rootOf("A"), approvalOf("B", "A"))
        val n = InstallTrustKeeper(server, EnetTrustCrypto, persistence, { identity })
        assertEquals(false, n.store(account)?.existing)
        n.sync(account, "install-N")
        assertEquals(InstallTrustState.Waiting, n.snapshot(account).state)
        assertEquals(listOf("install-N"), server.asked)
        // Next to older builds only (no chain, not alone) it waits too, never adopts.
        val older = AccountServer(listOf(stripped("A"), fx.served("key" to "N")))
        val other = TrustStorePersistence.Memory()
        InstallTrustKeeper.markNew(account, identity, EnetTrustCrypto, other)
        val fresh = InstallTrustKeeper(older, EnetTrustCrypto, other, { identity })
        fresh.sync(account, "install-N")
        assertEquals(InstallTrustState.Waiting, fresh.snapshot(account).state)
        assertEquals(emptyList<String>(), older.claims)
    }

    /** [InstallTrustKeeper.markNew] keeps a store this endpoint already keeps,
     *  replaces one kept for another endpoint (a rotated identity) and never
     *  writes over a store it cannot read. */
    @Test
    fun markNewKeepsItsOwnStoreAndNeverWritesOverAnUnreadableOne() {
        val persistence = TrustStorePersistence.Memory()
        val kept = fx.namedStore("victim").store.copy(existing = false)
        persistence.save(account, storeJson(kept))
        InstallTrustKeeper.markNew(account, fx.identity("A"), EnetTrustCrypto, persistence)
        assertEquals(kept, InstallTrustKeeper.json.decodeFromString(TrustStore.serializer(), persistence.load(account)!!))
        InstallTrustKeeper.markNew(account, fx.identity("N"), EnetTrustCrypto, persistence)
        assertEquals(TrustStore(fx.endpoint("N"), ""), InstallTrustKeeper.json.decodeFromString(TrustStore.serializer(), persistence.load(account)!!))
        var saved = 0
        val unreadable = object : TrustStorePersistence {
            override fun load(accountId: String): String = throw IllegalStateException("unreadable")
            override fun save(accountId: String, json: String): Boolean {
                saved++
                return true
            }
            override fun remove(accountId: String) {}
        }
        InstallTrustKeeper.markNew(account, fx.identity("N"), EnetTrustCrypto, unreadable)
        assertEquals(0, saved)
    }

    /** A store that cannot be read is never taken for none: no adoption, no
     *  claim, no ask, nothing written over it. The Keystore vault throws for a
     *  slot that does not open, and answers none only for no slot. */
    @Test
    fun anUnreadableStoreIsNeverTakenForNone() = runBlocking {
        var saved = 0
        val persistence = object : TrustStorePersistence {
            override fun load(accountId: String): String = throw IllegalStateException("unreadable")
            override fun save(accountId: String, json: String): Boolean {
                saved++
                return true
            }
            override fun remove(accountId: String) {}
        }
        val server = AccountServer(listOf(fx.served("key" to "A"), fx.served("key" to "B")))
        val identity = fx.identity("A")
        val keeper = InstallTrustKeeper(server, EnetTrustCrypto, persistence, { identity })
        assertEquals(false, keeper.store(account)?.existing)
        keeper.sync(account, "install-A")
        keeper.record(account, root0, 1, KeyRecord.Verified)
        assertEquals(emptyList<String>(), server.claims)
        assertEquals(emptyList<String>(), server.asked)
        assertEquals(0, saved)
        // The vault: no slot = none; a slot that does not open throws.
        val slots = MapSlots()
        val broken = object : SecretSealer {
            override fun seal(plain: ByteArray) = plain
            override fun open(blob: ByteArray): ByteArray? = null
        }
        val vault = TrustVault(slots, broken)
        assertNull(vault.load(account))
        vault.save(account, "{}")
        assertTrue(runCatching { vault.load(account) }.isFailure)
        val good = TrustVault(MapSlots(), XorSealer)
        good.save(account, storeJson(fx.namedStore("victim").store))
        assertEquals(fx.namedStore("victim").store, InstallTrustKeeper.json.decodeFromString(TrustStore.serializer(), good.load(account)!!))
    }

    /** A kept store that does not decode, or one kept for another endpoint
     *  (a rotated identity), is a new install's: never existing. */
    @Test
    fun aStoreOfAnotherEndpointOrThatDoesNotDecodeIsNew() {
        val identity = fx.identity("A")
        for (data in listOf("{", storeJson(TrustStore(fx.endpoint("N"), "user-me", existing = true)))) {
            val persistence = TrustStorePersistence.Memory()
            persistence.save(account, data)
            val keeper = InstallTrustKeeper(AccountServer(emptyList()), EnetTrustCrypto, persistence, { identity })
            assertFalse(keeper.store(account)!!.existing)
        }
    }
}
