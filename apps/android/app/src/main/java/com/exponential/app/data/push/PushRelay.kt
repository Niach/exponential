package com.exponential.app.data.push

import android.util.Log
import com.exponential.app.data.auth.SecureStore
import io.ktor.client.HttpClient
import io.ktor.client.request.delete
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.put
import io.ktor.client.request.setBody
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/**
 * EXP-1435: the push relay's accountless API (apps/push-relay/README.md). The
 * device registers ONE install per relay with its FCM token, mints ONE
 * capability per (relay, signed-in account) and hands that capability plus its
 * X25519 public key to the account's server, which then seals every push to
 * this device; the relay forwards `{s: <sealed>}` as a data-only FCM message.
 */
internal sealed interface RelayResult<out T> {
    data class Ok<T>(val value: T) : RelayResult<T>

    /** The install is gone (401 bad secret / 404 unknown install): forget it and register again. */
    data object Gone : RelayResult<Nothing>

    /** Network, 4xx/5xx (503 = the relay has no FCM credentials): unusable for this pass. */
    data class Failed(val reason: String) : RelayResult<Nothing>
}

internal data class RelayInstallCreds(val installId: String, val installSecret: String)

@Serializable
internal data class RelayCapabilityRecord(val id: String, val capability: String)

/** One install on one relay, with the FCM token it holds and the capability minted per account id. */
@Serializable
internal data class RelayInstallRecord(
    val installId: String,
    val installSecret: String,
    val token: String,
    val capabilities: Map<String, RelayCapabilityRecord> = emptyMap(),
)

internal interface PushRelayOps {
    suspend fun createInstall(relayUrl: String, token: String): RelayResult<RelayInstallCreds>
    suspend fun updateInstall(relayUrl: String, install: RelayInstallRecord, token: String): RelayResult<Unit>
    suspend fun listCapabilities(relayUrl: String, install: RelayInstallRecord): RelayResult<Set<String>>
    suspend fun mintCapability(relayUrl: String, install: RelayInstallRecord, label: String): RelayResult<RelayCapabilityRecord>
    suspend fun revokeCapability(relayUrl: String, install: RelayInstallRecord, capabilityId: String): RelayResult<Unit>
}

/** Installs keyed by relay base URL. */
internal interface RelayInstallStore {
    fun load(): Map<String, RelayInstallRecord>
    fun save(installs: Map<String, RelayInstallRecord>)
}

/** The relay's install routes over the app's shared ktor/OkHttp client. */
internal class HttpPushRelayOps(
    private val client: HttpClient,
    private val json: Json,
) : PushRelayOps {
    override suspend fun createInstall(relayUrl: String, token: String): RelayResult<RelayInstallCreds> =
        call("register") {
            client.post("$relayUrl/installs") {
                contentType(ContentType.Application.Json)
                setBody(buildJsonObject { put("platform", "fcm"); put("token", token) }.toString())
            }
        }.map { body ->
            val obj = json.parseToJsonElement(body).jsonObject
            RelayInstallCreds(
                installId = obj.string("installId") ?: error("no installId"),
                installSecret = obj.string("installSecret") ?: error("no installSecret"),
            )
        }

    override suspend fun updateInstall(relayUrl: String, install: RelayInstallRecord, token: String): RelayResult<Unit> =
        call("rotate", installRoute = true) {
            client.put("$relayUrl/installs/${install.installId}") {
                header("Authorization", "Bearer ${install.installSecret}")
                contentType(ContentType.Application.Json)
                setBody(buildJsonObject { put("token", token) }.toString())
            }
        }.map { }

    override suspend fun listCapabilities(relayUrl: String, install: RelayInstallRecord): RelayResult<Set<String>> =
        call("list", installRoute = true) {
            client.get("$relayUrl/installs/${install.installId}/capabilities") {
                header("Authorization", "Bearer ${install.installSecret}")
            }
        }.map { body ->
            val list = json.parseToJsonElement(body).jsonObject["capabilities"] as? JsonArray ?: error("no capabilities")
            list.mapNotNull { (it as? JsonObject)?.string("capabilityId") }.toSet()
        }

    override suspend fun mintCapability(
        relayUrl: String,
        install: RelayInstallRecord,
        label: String,
    ): RelayResult<RelayCapabilityRecord> =
        call("mint", installRoute = true) {
            client.post("$relayUrl/installs/${install.installId}/capabilities") {
                header("Authorization", "Bearer ${install.installSecret}")
                contentType(ContentType.Application.Json)
                setBody(buildJsonObject { put("label", label) }.toString())
            }
        }.map { body ->
            val obj = json.parseToJsonElement(body).jsonObject
            RelayCapabilityRecord(
                id = obj.string("capabilityId") ?: error("no capabilityId"),
                capability = obj.string("capability") ?: error("no capability"),
            )
        }

    override suspend fun revokeCapability(
        relayUrl: String,
        install: RelayInstallRecord,
        capabilityId: String,
    ): RelayResult<Unit> =
        call("revoke", installRoute = true) {
            client.delete("$relayUrl/installs/${install.installId}/capabilities/$capabilityId") {
                header("Authorization", "Bearer ${install.installSecret}")
            }
        }.map { }

    private suspend fun call(
        op: String,
        installRoute: Boolean = false,
        request: suspend () -> HttpResponse,
    ): RelayResult<String> = try {
        val response = request()
        val status = response.status.value
        when {
            response.status.isSuccess() -> RelayResult.Ok(response.bodyAsText())
            installRoute && (status == 401 || status == 404) -> RelayResult.Gone
            else -> RelayResult.Failed("relay $op HTTP $status")
        }
    } catch (err: CancellationException) {
        throw err
    } catch (err: Exception) {
        RelayResult.Failed("relay $op ${err.javaClass.simpleName}")
    }

    private inline fun <T> RelayResult<String>.map(parse: (String) -> T): RelayResult<T> = when (this) {
        is RelayResult.Ok -> try {
            RelayResult.Ok(parse(value))
        } catch (err: Exception) {
            RelayResult.Failed("relay body ${err.javaClass.simpleName}")
        }
        RelayResult.Gone -> RelayResult.Gone
        is RelayResult.Failed -> this
    }

    private fun JsonObject.string(key: String): String? = (this[key] as? JsonPrimitive)?.contentOrNull
}

/** Installs persisted in the app's encrypted prefs: the install secret IS a credential. */
internal class SecureRelayInstallStore(
    private val secureStore: SecureStore,
    private val json: Json,
) : RelayInstallStore {
    private val serializer = MapSerializer(String.serializer(), RelayInstallRecord.serializer())

    override fun load(): Map<String, RelayInstallRecord> =
        secureStore.get(KEY)?.let { runCatching { json.decodeFromString(serializer, it) }.getOrNull() }.orEmpty()

    override fun save(installs: Map<String, RelayInstallRecord>) {
        secureStore.set(KEY, if (installs.isEmpty()) null else json.encodeToString(serializer, installs))
    }

    private companion object {
        const val KEY = "push.relayInstalls.v1"
    }
}

/**
 * Keeps one install per relay and one capability per (relay, account). Every
 * relay failure answers null ("register the bare token this pass"); the
 * reconcile loop retries. Serialised by one mutex: the reconcile loop and a
 * sign-out's revoke both rewrite the stored installs.
 */
internal class PushRelayRegistrar(
    private val ops: PushRelayOps,
    private val store: RelayInstallStore,
) {
    private val mutex = Mutex()

    // Relays whose stored install was checked against the relay this process:
    // a relay that lost its database (no persistent volume) keeps answering
    // 401/404 for an install the device still believes in, and a capability
    // the server holds would then 401 on every push.
    private val verified = mutableSetOf<String>()

    /** The capability for [accountId] on [relayUrl], its install holding [token]; null = relay unusable now. */
    suspend fun capabilityFor(relayUrl: String, accountId: String, token: String, label: String): String? =
        mutex.withLock {
            val installs = store.load().toMutableMap()
            var install = installs[relayUrl]

            if (install != null && relayUrl !in verified) {
                when (val listed = ops.listCapabilities(relayUrl, install)) {
                    is RelayResult.Ok -> {
                        install = install.copy(capabilities = install.capabilities.filterValues { it.id in listed.value })
                        verified += relayUrl
                    }
                    RelayResult.Gone -> install = null
                    is RelayResult.Failed -> return@withLock fail(listed.reason)
                }
                installs.record(relayUrl, install)
            }

            if (install != null && install.token != token) {
                when (val rotated = ops.updateInstall(relayUrl, install, token)) {
                    is RelayResult.Ok -> install = install.copy(token = token)
                    RelayResult.Gone -> install = null
                    is RelayResult.Failed -> return@withLock fail(rotated.reason)
                }
                installs.record(relayUrl, install)
            }

            if (install == null) {
                when (val created = ops.createInstall(relayUrl, token)) {
                    is RelayResult.Ok -> {
                        install = RelayInstallRecord(created.value.installId, created.value.installSecret, token)
                        verified += relayUrl
                        installs[relayUrl] = install
                        store.save(installs)
                    }
                    RelayResult.Gone -> return@withLock fail("relay register refused")
                    is RelayResult.Failed -> return@withLock fail(created.reason)
                }
            }

            install.capabilities[accountId]?.let { return@withLock it.capability }
            when (val minted = ops.mintCapability(relayUrl, install, label.take(MAX_LABEL))) {
                is RelayResult.Ok -> {
                    installs[relayUrl] = install.copy(capabilities = install.capabilities + (accountId to minted.value))
                    store.save(installs)
                    minted.value.capability
                }
                RelayResult.Gone -> {
                    installs.remove(relayUrl)
                    store.save(installs)
                    fail("relay install gone")
                }
                is RelayResult.Failed -> fail(minted.reason)
            }
        }

    /** Sign-out: revoke [accountId]'s capabilities (best effort) and forget them either way. */
    suspend fun revoke(accountId: String) {
        mutex.withLock {
            val installs = store.load().toMutableMap()
            var changed = false
            for ((relayUrl, install) in installs.toMap()) {
                val capability = install.capabilities[accountId] ?: continue
                val result = ops.revokeCapability(relayUrl, install, capability.id)
                if (result is RelayResult.Failed) Log.w(TAG, "capability revoke failed: ${result.reason}")
                installs[relayUrl] = install.copy(capabilities = install.capabilities - accountId)
                changed = true
            }
            if (changed) store.save(installs)
        }
    }

    // Writes the in-progress map before failing, so a forgotten install or a
    // rotated token is not re-litigated on the next pass.
    private fun MutableMap<String, RelayInstallRecord>.record(relayUrl: String, install: RelayInstallRecord?) {
        if (install == null) remove(relayUrl) else this[relayUrl] = install
        store.save(this)
    }

    private fun fail(reason: String): String? {
        Log.w(TAG, "push relay unusable this pass: $reason")
        return null
    }

    private companion object {
        const val TAG = "PushRelay"
        const val MAX_LABEL = 64
    }
}

/** What one account's registration posts: sealed pushes need both, legacy neither. */
internal data class SealedRegistration(val capability: String, val pushPublicKey: String)

/**
 * One account's registration decision (EXP-1435), over function seams:
 *  - the server names no relay (or predates `pushTokens.relay`), or this build
 *    cannot open sealed pushes (no native facade) → the bare token, done;
 *  - a relay and a capability → the token + capability + public key, done;
 *  - the relay lookup or the relay itself failed → the bare token so pushes keep
 *    flowing through the /send shim, but NOT done: the loop retries.
 * Returns whether the registration is complete; a register failure throws.
 */
internal class PushAccountRegistrar(
    private val relayUrl: suspend (accountId: String) -> String?,
    private val publicKey: () -> String?,
    private val capabilityFor: suspend (relayUrl: String, accountId: String, token: String) -> String?,
    private val register: suspend (accountId: String, token: String, sealed: SealedRegistration?) -> Unit,
) {
    suspend fun register(accountId: String, token: String): Boolean {
        var complete = true
        val relay = try {
            relayUrl(accountId)
        } catch (err: CancellationException) {
            throw err
        } catch (err: Exception) {
            Log.w(TAG, "pushTokens.relay failed: ${err.javaClass.simpleName}")
            complete = false
            null
        }
        val sealed = relay?.let { url ->
            val key = publicKey() ?: return@let null
            val capability = capabilityFor(url, accountId, token)
            if (capability == null) {
                complete = false
                null
            } else {
                SealedRegistration(capability, key)
            }
        }
        register(accountId, token, sealed)
        return complete
    }

    private companion object {
        const val TAG = "PushTokenMgr"
    }
}
