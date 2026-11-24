package com.exponential.app.data.sealed

import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.enet.EnetFfi
import com.exponential.app.data.enet.base64Url
import com.exponential.app.data.enet.hexBytes
import com.exponential.app.domain.contractFixtureJson
import com.exponential.app.domain.InstallApprovalView
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1950: packages/domain-contract/fixtures/install-trust.json, EVERY case
 * by name: the constants, the keys and their registrations, the code vectors,
 * the approval vector byte-exact, `trustCases` (reconcile), the four rules
 * (`keepCases`, `sealCases`, `targetCases`, `callCases`) now AND once
 * mandatory, `maliciousServer` against its rules, `newRoot`. The ed25519 runs
 * through the FULL eNet binding ([EnetTrustCrypto]: `identityVerify`,
 * `identitySign`, `acceptWrap`, `acceptVouch`; the host build of
 * exponential-net-ffi, loaded by JNA from `jna.library.path`); the same
 * replay runs in TS (src/install-trust.test.ts) and Swift.
 */
class InstallTrustFixtureTest {
    private val fx = TrustFixture()
    private val f = fx.f
    private val crypto = EnetTrustCrypto

    private fun at(c: JsonObject, mode: TrustMode) = c.obj(if (mode == TrustMode.Record) "now" else "mandatory")

    // ── the constants ─────────────────────────────────────────────────────

    @Test
    fun theCopyTheIconAndTheMode() {
        val copy = f.obj("copy")
        assertEquals(copy.str("newInstall"), InstallApprovalView.Copy.NEW_INSTALL)
        assertEquals(copy.str("reset"), InstallApprovalView.Copy.RESET)
        assertEquals(copy.str("approve"), InstallApprovalView.Copy.APPROVE)
        assertEquals(copy.str("remove"), InstallApprovalView.Copy.REMOVE)
        assertEquals(copy.str("waiting"), InstallApprovalView.Copy.WAITING)
        assertEquals(f.str("icon"), InstallApprovalView.ICON)
        val mode = f.obj("mode")
        assertEquals(mode.str("release1"), TrustMode.BUILD.wire)
        assertEquals(mode.str("release2"), TrustMode.Mandatory.wire)
        val approval = f.obj("approval")
        assertEquals(approval.str("label"), InstallApproval.LABEL)
        assertEquals(approval["messageBytes"]!!.jsonPrimitive.int, InstallApproval.MESSAGE_BYTES)
        val kinds = approval.obj("kinds")
        for (kind in ApprovalKind.entries) assertEquals(kind.wire, kinds[kind.wire]!!.jsonPrimitive.int, kind.byte)
        assertEquals(ApprovalKind.entries.size, kinds.size)
    }

    @Test
    fun theCodeVectorsAndEveryKeysCode() {
        val code = f.obj("code")
        assertEquals(code.str("label"), InstallCode.LABEL)
        assertEquals(code.str("alphabet"), InstallCode.ALPHABET)
        assertEquals(50, code["bits"]!!.jsonPrimitive.int)
        val vectors = code.cases("vectors")
        assertEquals(5, vectors.size)
        for (v in vectors) assertEquals(v.str("code"), InstallCode.of(v.str("endpointId")))
        for ((name, key) in fx.keys) assertEquals(name, key.jsonObject.str("code"), InstallCode.of(key.jsonObject.str("endpointId")))
        assertNull(InstallCode.of("not hex"))
        assertNull(InstallCode.of("A".repeat(64)))
    }

    @Test
    fun theApprovalMessageByteExact() {
        val v = f.obj("approval").obj("vector")
        val subject = v.obj("subject")
        val message = InstallApproval.message(ApprovalKind.of(v.str("kind"))!!, subject.str("userId"), subject.str("endpointId"), subject.str("x25519Public"))!!
        assertArrayEquals(v.str("message").hexBytes(), message)
        assertEquals(InstallApproval.MESSAGE_BYTES, message.size)
        assertNull(InstallApproval.message(ApprovalKind.Approve, "", subject.str("endpointId"), subject.str("x25519Public")))
        assertNull(InstallApproval.message(ApprovalKind.Approve, "u", "zz", subject.str("x25519Public")))
    }

    /** The capture specimen's rows are the fixture's keys N and T3. */
    @Test
    fun theSpecimenRowsAreTheFixtures() {
        for ((row, name) in InstallApprovalView.SPECIMEN_ROWS.zip(listOf("N", "T3"))) {
            val key = fx.key(name)
            assertEquals(name, key.str("endpointId"), row.endpointId)
            assertEquals(name, key.str("userId"), row.userId)
            assertEquals(name, key.str("label"), row.label)
            assertEquals(name, key.str("code"), row.code)
            assertEquals(name, InstallCode.of(row.endpointId), row.code)
        }
    }

    /** The trees web draws (install-approval.tsx, the fixture's `row` on #3416): ids, components, props. */
    @Test
    fun theTreesAreWebs() {
        val row = InstallApprovalView.SPECIMEN_ROWS[1]
        val tree = InstallApprovalView.pendingTree(listOf(row to "Mira's new iPhone"))
        assertEquals("pending-installs", tree.str("id"))
        assertEquals("Section", tree.str("component"))
        val item = tree.cases("children").single()
        assertEquals("install-install-T3", item.str("id"))
        assertEquals("Stack", item.str("component"))
        assertEquals(mapOf("direction" to "vertical", "gap" to "sm"), item.obj("props").strings())
        val lines = item.cases("children")
        assertEquals(listOf("Row", "Stack"), lines.map { it.str("component") })
        val r = lines[0]
        assertEquals("install-install-T3-row", r.str("id"))
        assertEquals(mapOf("title" to "Mira's new iPhone", "subtitle" to "776AF T3NNF"), r.obj("props").strings())
        // The Row's trailing slot is the Badge alone: no Button beside the code.
        val slots = r.obj("slots")
        assertEquals(setOf("trailing"), slots.keys)
        val badge = slots.obj("trailing")
        assertEquals("install-install-T3-badge", badge.str("id"))
        assertEquals("Badge", badge.str("component"))
        assertNull(badge["children"])
        assertEquals(mapOf("text" to "Reset", "variant" to "secondary"), badge.obj("props").strings())
        assertFalse(r.toString().contains("\"Button\""))
        // Under it the actions, Approve then Remove.
        val actions = lines[1]
        assertEquals("install-install-T3-actions", actions.str("id"))
        assertEquals(mapOf("direction" to "horizontal", "gap" to "sm"), actions.obj("props").strings())
        val parts = actions.cases("children")
        assertEquals(listOf("Button", "Button"), parts.map { it.str("component") })
        assertEquals(listOf("install-install-T3-approve", "install-install-T3-remove"), parts.map { it.str("id") })
        assertEquals(mapOf("label" to "Approve", "size" to "sm"), parts[0].obj("props").strings())
        assertEquals(mapOf("label" to "Remove", "size" to "sm", "variant" to "ghost"), parts[1].obj("props").strings())
        val waiting = InstallApprovalView.waitingTree("HNMBN A0319")
        assertEquals("install-waiting", waiting.str("id"))
        assertEquals("EmptyState", waiting.str("component"))
        assertEquals(mapOf("icon" to "ui-private", "title" to "Waiting for approval", "description" to "HNMBN A0319"), waiting.obj("props").strings())
        // A row's title: the label; a reset leads with the member's name; nothing = the code.
        assertEquals("Mira · Mira's new iPhone", InstallApprovalView.title(row, "Mira"))
        assertEquals(row.code, InstallApprovalView.title(row.copy(kind = TrustRow.Kind.NewInstall, label = null), null))
    }

    @Test
    fun theNewRootRule() {
        val block = f.obj("newRoot")
        assertEquals(KeyRecord.Verified.wire, block.str("makerRecord"))
        assertEquals(mapOf("now" to "unsigned", "mandatory" to "refused"), block.obj("readerRecordBeforeResign").strings())
        assertEquals("verified", block.str("readerRecordAfterResign"))
    }

    // ── the keys ──────────────────────────────────────────────────────────

    @Test
    fun everyKeyAndItsRegistration() {
        for (name in fx.keys.keys.sorted()) {
            val identity = fx.identity(name)
            assertEquals(name, fx.endpoint(name), crypto.endpointId(identity))
            assertEquals(name, fx.key(name).str("x25519Public"), base64Url(EnetFfi.x25519PublicOf(identity)))
            val install = fx.served(spec("key" to name))
            assertTrue(name, InstallTrust.registrationValid(install, crypto))
            val message = InstallTrust.registrationMessage(install.userId, install.endpointId, install.x25519Public, fx.key(name).str("signedAt"))
            assertEquals(name, install.registrationSignature, crypto.sign(identity, message.toByteArray()))
            assertEquals(name, name == "Z", InstallTrust.registrationValid(fx.served(spec("key" to name, "registration" to "forged")), crypto))
            assertFalse(name, InstallTrust.registrationValid(fx.served(spec("key" to name, "registration" to "none")), crypto))
            // Another X25519 key than the registration binds.
            val other = fx.key(if (name == "Z") "A" else "Z").str("x25519Public")
            assertFalse(name, InstallTrust.registrationValid(fx.served(spec("key" to name, "x25519Public" to other)), crypto))
        }
    }

    @Test
    fun theApprovalVectorSignsAndVerifies() {
        val v = f.obj("approval").obj("vector")
        val s = v.obj("subject")
        val subject = ServedInstall("x", s.str("userId"), s.str("endpointId"), s.str("x25519Public"))
        val signature = v.str("signature")
        val approver = v.str("approverEndpointId")
        assertEquals(signature, InstallApproval.sign(ApprovalKind.Approve, subject, fx.identity("A"), crypto))
        assertTrue(InstallApproval.verify(ApprovalKind.Approve, subject, signature, approver, crypto))
        // Every field is covered: another kind, user, endpoint or key fails.
        assertFalse(InstallApproval.verify(ApprovalKind.Reset, subject, signature, approver, crypto))
        assertFalse(InstallApproval.verify(ApprovalKind.Approve, subject.copy(userId = "user-other"), signature, approver, crypto))
        assertFalse(InstallApproval.verify(ApprovalKind.Approve, subject.copy(endpointId = fx.endpoint("B")), signature, approver, crypto))
        assertFalse(InstallApproval.verify(ApprovalKind.Approve, subject.copy(x25519Public = fx.key("B").str("x25519Public")), signature, approver, crypto))
        assertFalse(InstallApproval.verify(ApprovalKind.Approve, subject, signature, fx.endpoint("B"), crypto))
        // A malformed signature never verifies (and never throws).
        assertFalse(crypto.verify(signature.dropLast(1), ByteArray(0), approver))
        assertFalse(crypto.verify(signature, ByteArray(0), approver.uppercase()))
    }

    // ── reconcile ─────────────────────────────────────────────────────────

    @Test
    fun replaysEveryTrustCase() {
        val all = f.cases("trustCases")
        assertEquals(34, all.size)
        for (c in all) {
            val name = c.str("name")
            val answer = TrustAnswer(
                installs = c.cases("installs").map { fx.served(it) },
                approvals = c.cases("approvals").map { a ->
                    ServedApproval(ApprovalKind.of(a.str("kind"))!!, fx.endpoint(a.str("subject")), fx.endpoint(a.str("approver")), a.str("signature"))
                },
            )
            val view = InstallTrust.reconcile(fx.store(c.obj("before")), answer, crypto)
            assertEquals(name, c.str("status"), view.status.wire)
            val after = c.obj("after")
            assertEquals(name, after["joined"]!!.jsonPrimitive.boolean, view.store.joined)
            val pins = view.store.pins.map { (endpointId, pin) -> fx.nameOf(endpointId) to listOf(pin.via.wire, pin.by?.let(fx::nameOf)) }.toMap()
            val expected = after.obj("pins").mapValues { (_, pair) -> pair.jsonArray.map { (it as? JsonPrimitive)?.contentOrNull } }
            assertEquals(name, expected, pins)
            assertEquals(name, after.strings("members"), view.store.members)
            assertEquals(name, after.strings("declined"), view.store.declined.map { fx.nameOf(it) ?: it })
            val rows = c.cases("rows").map { r -> listOf(r.str("kind"), r.str("key"), r.str("installId"), r.str("code"), r.strOrNull("label") ?: "<nil>") }
            assertEquals(name, rows, view.rows.map { listOf(it.kind.wire, fx.nameOf(it.endpointId) ?: it.endpointId, it.installId, it.code, it.label ?: "<nil>") })
            assertEquals(name, c["approveExisting"]?.jsonArray?.map { it.jsonPrimitive.content } ?: emptyList<String>(), view.approveExisting.map { fx.nameOf(it) ?: it })
            after["existing"]?.jsonPrimitive?.booleanOrNull?.let { assertEquals(name, it, view.store.existing) }
        }
    }

    /** Decision (e): on day one no row and no waiting state for an account whose installs all existed. */
    @Test
    fun dayOneNoRowNoWaiting() {
        val dayOne = f.cases("trustCases").filter { it["dayOne"]?.jsonPrimitive?.booleanOrNull == true }
        assertTrue(dayOne.size >= 4)
        for (c in dayOne) {
            val name = c.str("name")
            assertEquals(name, 0, c.cases("rows").size)
            assertNotEquals(name, TrustStatus.Waiting.wire, c.str("status"))
            assertEquals(name, true, c.obj("after")["joined"]!!.jsonPrimitive.boolean)
        }
    }

    /** Decision (e): a planted install after the first reconcile is never pinned. */
    @Test
    fun aPlantedInstallAfterTheFirstReconcileIsNeverPinned() {
        val attacks = f.cases("trustCases").filter { it.strOrNull("attack") == "existing" }
        assertTrue(attacks.size >= 3)
        for (c in attacks) {
            val name = c.str("name")
            val before = c.obj("before")
            if (before["existing"]?.jsonPrimitive?.booleanOrNull == true) continue // the accepted moment: the first reconcile
            val pins = c.obj("after").obj("pins").keys.sorted()
            assertFalse(name, "Z" in pins)
            assertEquals(name, before.obj("pins").keys.sorted(), pins)
        }
        assertTrue(attacks.any { c -> c.cases("rows").any { it.str("key") == "Z" && it.str("kind") == "newInstall" } })
    }

    // ── the four rules, now and once mandatory ────────────────────────────

    @Test
    fun r1KeepWrapEveryCase() {
        val all = f.obj("rules").cases("keepCases")
        assertEquals(9, all.size)
        for (c in all) {
            val name = c.str("name")
            val named = fx.namedStore(c.str("store"))
            val wrap = fx.heldWrap(c.obj("wrap"))
            for (mode in TrustMode.entries) {
                val want = at(c, mode)
                val label = "$name ($mode)"
                when (val got = InstallTrust.keepWrap(wrap, named.identity, named.installSecret, named.store, mode, crypto)) {
                    is InstallTrust.KeepVerdict.Keep -> {
                        assertEquals(label, true, want["keep"]!!.jsonPrimitive.boolean)
                        assertEquals(label, want.str("record"), got.record.wire)
                    }
                    is InstallTrust.KeepVerdict.Refuse -> {
                        assertEquals(label, false, want["keep"]!!.jsonPrimitive.boolean)
                        assertEquals(label, want.str("reason"), got.reason.wire)
                    }
                }
            }
        }
    }

    @Test
    fun r2r3SealToEveryCase() {
        val all = f.obj("rules").cases("sealCases")
        assertEquals(11, all.size)
        for (c in all) {
            val name = c.str("name")
            val store = fx.namedStore(c.str("store")).store
            val held = c["held"]?.takeIf { it !is JsonNull }?.jsonObject?.let { it.str("secret").hexBytes() to KeyRecord.of(it.str("record"))!! }
            val vouch = c["vouch"]?.takeIf { it !is JsonNull }?.jsonObject?.let(fx::vouch)
            for (mode in TrustMode.entries) {
                val want = at(c, mode)
                val label = "$name ($mode)"
                when (val got = InstallTrust.sealTo(c.str("rootId"), c["epoch"]!!.jsonPrimitive.int, c.str("publicKey"), held, vouch, store, mode, crypto)) {
                    is InstallTrust.SealVerdict.Seal -> {
                        assertEquals(label, true, want["seal"]!!.jsonPrimitive.boolean)
                        assertEquals(label, want.str("record"), got.record.wire)
                    }
                    is InstallTrust.SealVerdict.Refuse -> {
                        assertEquals(label, false, want["seal"]!!.jsonPrimitive.boolean)
                        assertEquals(label, want.str("reason"), got.reason.wire)
                    }
                }
            }
        }
    }

    @Test
    fun r4WrapTargetEveryCase() {
        val all = f.obj("rules").cases("targetCases")
        assertEquals(14, all.size)
        // R4 runs against every named store (`adopted` = one that pinned its account's installs `existing`).
        assertEquals(setOf("victim", "waiting", "adopted"), all.map { it.str("store") }.toSet())
        for (c in all) {
            val name = c.str("name")
            val store = fx.namedStore(c.str("store")).store
            val roots = c["roots"]!!.jsonArray.map { fx.endpoint(it.jsonPrimitive.content) }.toSet()
            val target = fx.served(c.obj("target"))
            for (mode in TrustMode.entries) {
                val want = at(c, mode)
                val label = "$name ($mode)"
                when (val got = InstallTrust.wrapTarget(target, store, mode, roots, crypto)) {
                    is InstallTrust.TargetVerdict.Wrap -> {
                        assertEquals(label, true, want["wrap"]!!.jsonPrimitive.boolean)
                        assertEquals(label, want.str("record"), got.record.wire)
                    }
                    is InstallTrust.TargetVerdict.Refuse -> {
                        assertEquals(label, false, want["wrap"]!!.jsonPrimitive.boolean)
                        assertEquals(label, want.str("reason"), got.reason.wire)
                    }
                }
            }
        }
    }

    @Test
    fun callKeyEveryCase() {
        val all = f.obj("rules").cases("callCases")
        assertEquals(6, all.size)
        for (c in all) {
            val name = c.str("name")
            val store = fx.namedStore(c.str("store")).store
            val target = c.obj("target")
            val held = c["held"]?.takeIf { it !is JsonNull }?.jsonObject?.let { it.str("secret").hexBytes() to KeyRecord.of(it.str("record"))!! }
            for (mode in TrustMode.entries) {
                val want = at(c, mode)
                val label = "$name ($mode)"
                when (val got = InstallTrust.callKey(target.str("rootId"), target["epoch"]!!.jsonPrimitive.int, held, store, mode)) {
                    is InstallTrust.CallKeyVerdict.Key -> {
                        assertEquals(label, true, want["key"]!!.jsonPrimitive.boolean)
                        assertEquals(label, want.str("record"), got.record.wire)
                        assertArrayEquals(label, held!!.first, got.secret)
                    }
                    InstallTrust.CallKeyVerdict.NoKey -> assertEquals(label, "noKey", want.str("noKey"))
                    InstallTrust.CallKeyVerdict.Rollback -> assertEquals(label, "rollback", want.str("noKey"))
                }
            }
        }
    }

    /** Every attack case names its rule's case of the same name with the same
     *  verdicts, and once mandatory none keeps, seals, wraps or keys anything. */
    @Test
    fun maliciousServerCasesAreTheirRulesCases() {
        val rules = f.obj("rules")
        val entries = f.cases("maliciousServer")
        assertEquals(listOf(1, 2, 3, 4), entries.map { it["attack"]!!.jsonPrimitive.int })
        var count = 0
        for (entry in entries) {
            for (c in entry.cases("cases")) {
                count++
                val name = c.str("name")
                val source = rules.cases(c.str("replay")).first { it.str("name") == name }
                assertEquals(name, entry["attack"]!!.jsonPrimitive.int, source["attack"]?.jsonPrimitive?.intOrNull)
                assertEquals(name, source["now"], c["now"])
                assertEquals(name, source["mandatory"], c["mandatory"])
                assertTrue(name, c.obj("answers").isNotEmpty())
                val mandatory = c.obj("mandatory")
                assertEquals(name, false, (mandatory["keep"] ?: mandatory["seal"] ?: mandatory["wrap"] ?: mandatory["key"])!!.jsonPrimitive.boolean)
            }
        }
        assertTrue(count > 0)
    }

    private fun spec(vararg pairs: Pair<String, Any>): JsonObject = JsonObject(
        pairs.associate { (k, v) -> k to if (v is Boolean) JsonPrimitive(v) else JsonPrimitive(v.toString()) },
    )
}

// ── the fixture's inputs, built the way src/install-trust.test.ts builds them ──

internal fun JsonObject.obj(key: String) = this[key]!!.jsonObject
internal fun JsonObject.str(key: String) = this[key]!!.jsonPrimitive.content
internal fun JsonObject.strOrNull(key: String) = (this[key] as? JsonPrimitive)?.contentOrNull
internal fun JsonObject.cases(key: String) = this[key]!!.jsonArray.map { it.jsonObject }
internal fun JsonObject.strings(key: String) = (this[key] as? JsonArray)?.map { it.jsonPrimitive.content } ?: emptyList()
internal fun JsonObject.strings(): Map<String, String> = mapValues { it.value.jsonPrimitive.content }

/** The replay and the malicious-server suite share these. */
internal class TrustFixture {
    val f: JsonObject = Json.parseToJsonElement(contractFixtureJson("install-trust.json")).jsonObject
    val keys: JsonObject = f.obj("keys")

    fun key(name: String): JsonObject = keys.obj(name)
    fun endpoint(name: String): String = key(name).str("endpointId")
    fun identity(name: String): ByteArray = key(name).str("identity").hexBytes()
    fun installSecret(name: String): ByteArray = key(name).str("installSecret").hexBytes()
    fun nameOf(endpointId: String): String? = keys.entries.firstOrNull { it.value.jsonObject.str("endpointId") == endpointId }?.key

    /** An install spec (`key`, `approvalRequested`, `registration`, `x25519Public`) as served. */
    fun served(spec: JsonObject): ServedInstall {
        val name = spec.str("key")
        val key = key(name)
        val registration = spec.strOrNull("registration")
        val signedAt = key.str("signedAt")
        val x25519 = key.str("x25519Public")
        var signature: String? = key.str("registrationSignature")
        if (registration == "none") signature = null
        if (registration == "forged") {
            // The server's install Z signed the registration message.
            val message = InstallTrust.registrationMessage(key.str("userId"), endpoint(name), x25519, signedAt)
            signature = EnetTrustCrypto.sign(identity("Z"), message.toByteArray())
        }
        return ServedInstall(
            installId = "install-$name",
            userId = key.str("userId"),
            endpointId = endpoint(name),
            x25519Public = spec.strOrNull("x25519Public") ?: x25519,
            registrationSignature = signature,
            registrationSignedAt = if (registration == "none") null else signedAt,
            approvalRequested = (spec["approvalRequested"] as? JsonPrimitive)?.booleanOrNull ?: false,
            label = key.strOrNull("label"),
        )
    }

    fun served(vararg pairs: Pair<String, Any>): ServedInstall = served(
        JsonObject(pairs.associate { (k, v) -> k to if (v is Boolean) JsonPrimitive(v) else JsonPrimitive(v.toString()) }),
    )

    /** A store spec (`viewer`, `joined`, `existing`, `pins`, `members`, `declined`, `roots`). */
    fun store(spec: JsonObject): TrustStore {
        val viewer = spec.str("viewer")
        val pins = LinkedHashMap<String, TrustPin>()
        for ((name, pin) in spec.obj("pins")) {
            val pair = pin.jsonArray
            pins[endpoint(name)] = TrustPin(
                userId = key(name).str("userId"),
                x25519Public = key(name).str("x25519Public"),
                via = PinVia.of(pair[0].jsonPrimitive.content)!!,
                by = (pair[1] as? JsonPrimitive)?.contentOrNull?.let(::endpoint),
            )
        }
        val roots = (spec["roots"] as? JsonObject)?.mapValues { (_, value) ->
            val v = value.jsonObject
            TrustRootRecord(v["verifiedEpoch"]!!.jsonPrimitive.int, v.obj("epochs").mapValues { KeyRecord.of(it.value.jsonPrimitive.content)!! })
        } ?: emptyMap()
        return TrustStore(
            selfEndpointId = endpoint(viewer),
            userId = key(viewer).str("userId"),
            joined = spec["joined"]!!.jsonPrimitive.boolean,
            existing = (spec["existing"] as? JsonPrimitive)?.booleanOrNull ?: false,
            pins = pins,
            members = spec.strings("members"),
            declined = spec.strings("declined").map(::endpoint),
            roots = roots,
        )
    }

    class Named(val store: TrustStore, val identity: ByteArray, val installSecret: ByteArray)

    fun namedStore(name: String): Named {
        val spec = f.obj("stores").obj(name)
        val viewer = spec.str("viewer")
        return Named(store(spec), identity(viewer), installSecret(viewer))
    }

    fun heldWrap(w: JsonObject) = HeldTreeKey(
        rootId = w.str("rootId"),
        epoch = w["epoch"]!!.jsonPrimitive.int,
        wrapped = w.str("wrapped"),
        signature = w.strOrNull("signature"),
        signerEndpointId = w.strOrNull("signerEndpointId"),
    )

    fun vouch(v: JsonObject) = Vouch(v.str("recipientEndpointId"), v.str("wrapped"), v.str("signature"), v.str("signerEndpointId"))

    @Suppress("unused")
    private fun JsonElement.asString() = jsonPrimitive.content
}
