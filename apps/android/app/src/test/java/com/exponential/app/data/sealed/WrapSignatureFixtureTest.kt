package com.exponential.app.data.sealed

import at.exponential.net.ffi.EnetException
import at.exponential.net.ffi.EnetServedWrap
import at.exponential.net.ffi.EnetVouchVerdict
import at.exponential.net.ffi.EnetWrapRefusal
import at.exponential.net.ffi.EnetWrapStatement
import at.exponential.net.ffi.EnetWrapVerdict
import at.exponential.net.ffi.acceptVouch
import at.exponential.net.ffi.acceptWrap
import at.exponential.net.ffi.endpointIdOf
import at.exponential.net.ffi.signWrap
import at.exponential.net.ffi.verifyWrapSignature
import at.exponential.net.ffi.wrapSignatureMessage
import com.exponential.app.data.enet.hexBytes
import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * #EXP-1950: packages/domain-contract/fixtures/wrap-signature.json through
 * the UniFFI binding (the host build of exponential-net-ffi, loaded by JNA
 * from `jna.library.path`): the vector, every `accepted`, `refused` and
 * `vouchCases` case by name. The same replay runs in TS, Rust and Swift.
 */
class WrapSignatureFixtureTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("wrap-signature.json")).jsonObject

    private fun JsonObject.obj(key: String) = this[key]!!.jsonObject
    private fun JsonObject.str(key: String) = this[key]!!.jsonPrimitive.content
    private fun JsonObject.strOrNull(key: String) = this[key]?.jsonPrimitive?.contentOrNull
    private fun JsonObject.hex(key: String) = str(key).hexBytes()
    private fun JsonObject.cases(key: String) = this[key]!!.jsonArray.map { it.jsonObject }
    private fun JsonObject.pins() = this["pins"]!!.jsonArray.map { it.jsonPrimitive.content }

    // One epoch above u32 stays malformed.
    private fun JsonObject.epoch() = this["epoch"]!!.jsonPrimitive.long.coerceAtMost(UInt.MAX_VALUE.toLong()).toUInt()

    private fun EnetWrapRefusal.spelled() = name.lowercase()

    private fun accept(case: JsonObject): EnetWrapVerdict = acceptWrap(
        wrap = EnetServedWrap(
            rootId = case.str("rootId"),
            epoch = case.epoch(),
            wrapped = case.str("wrapped"),
            signature = case.strOrNull("signature"),
            signerEndpointId = case.strOrNull("signerEndpointId"),
        ),
        identity = fixture.obj("recipient").hex("identity"),
        installSecret = fixture.obj("recipient").hex("installSecret"),
        pins = case.pins(),
        required = case["required"]!!.jsonPrimitive.boolean,
    )

    @Test
    fun theInstallsEndpointIdsAreTheFixtures() {
        for (install in listOf("signer", "recipient", "other")) {
            assertEquals(install, fixture.obj(install).str("endpointId"), endpointIdOf(fixture.obj(install).hex("identity")))
        }
    }

    @Test
    fun signsTheVectorByteExact() {
        val v = fixture.obj("vector")
        val statement = EnetWrapStatement(
            rootId = v.str("rootId"),
            epoch = v.epoch(),
            treePublicKey = v.str("treePublicKey"),
            recipientEndpointId = v.str("recipientEndpointId"),
            wrapped = v.str("wrapped"),
        )
        assertArrayEquals(v.hex("message"), wrapSignatureMessage(statement))
        val signature = signWrap(fixture.obj("signer").hex("identity"), statement)
        assertEquals(v.str("signature"), signature)
        val signer = fixture.obj("signer").str("endpointId")
        assertTrue(verifyWrapSignature(statement, signature, signer))
        assertFalse(verifyWrapSignature(statement, signature, fixture.obj("other").str("endpointId")))
        assertFalse(verifyWrapSignature(statement.copy(epoch = 2u), signature, signer))
        try {
            signWrap(fixture.obj("signer").hex("identity"), statement.copy(epoch = 0u))
            fail("epoch 0 signed")
        } catch (_: EnetException.Invalid) {
        }
    }

    @Test
    fun replaysAccepted() {
        val cases = fixture.cases("accepted")
        assertEquals(5, cases.size)
        for (case in cases) {
            val name = case.str("name")
            val (status, secret, public) = when (val got = accept(case)) {
                is EnetWrapVerdict.Verified -> {
                    assertEquals(name, case.str("signerEndpointId"), got.signerEndpointId)
                    Triple("verified", got.secret, got.treePublicKey)
                }
                is EnetWrapVerdict.Unsigned -> Triple("unsigned", got.secret, got.treePublicKey)
                is EnetWrapVerdict.Refused -> error("$name: refused ${got.reason.spelled()}")
            }
            assertEquals(name, case.str("status"), status)
            assertArrayEquals(name, fixture.obj("tree").hex("secret"), secret)
            assertEquals(name, fixture.obj("tree").str("publicKey"), public)
        }
    }

    @Test
    fun replaysRefusedEachWithItsReason() {
        val cases = fixture.cases("refused")
        assertEquals(20, cases.size)
        val seen = mutableSetOf<String>()
        for (case in cases) {
            val name = case.str("name")
            val got = accept(case) as? EnetWrapVerdict.Refused ?: error("$name: kept")
            assertEquals(name, case.str("reason"), got.reason.spelled())
            seen += got.reason.spelled()
        }
        assertEquals(fixture["refusals"]!!.jsonArray.map { it.jsonPrimitive.content }.toSet(), seen)
    }

    @Test
    fun replaysTheVouchCases() {
        val cases = fixture.cases("vouchCases")
        assertEquals(6, cases.size)
        for (case in cases) {
            val got = acceptVouch(
                statement = EnetWrapStatement(
                    rootId = case.str("rootId"),
                    epoch = case.epoch(),
                    treePublicKey = case.str("treePublicKey"),
                    recipientEndpointId = case.str("recipientEndpointId"),
                    wrapped = case.str("wrapped"),
                ),
                signature = case.strOrNull("signature"),
                signerEndpointId = case.strOrNull("signerEndpointId"),
                pins = case.pins(),
            )
            val result = when (got) {
                is EnetVouchVerdict.Verified -> "verified"
                is EnetVouchVerdict.Refused -> got.reason.spelled()
            }
            assertEquals(case.str("name"), case.str("result"), result)
        }
    }
}
