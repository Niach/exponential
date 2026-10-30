package com.exponential.app.data.sealed

import android.content.Context
import com.exponential.app.data.enet.IdentitySlots
import com.exponential.app.data.push.KeystoreSealer
import com.exponential.app.data.push.SecretSealer
import dagger.hilt.android.qualifiers.ApplicationContext
import java.util.Base64
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Tree secrets by root id, then epoch. */
typealias TreeKeys = Map<String, Map<Int, ByteArray>>

/**
 * EXP-1459: the tree keys every signed-in account of this install holds, in
 * memory, for the screens that open sealed content. Root ids are UUIDs, so
 * one map serves every account. The holder ([TreeKeyHolder]) fills it from
 * each account's [TreeKeyVault]; [missing] wakes the holder when a screen
 * met an epoch it holds no key for.
 */
object SealedKeys {
    private val state = MutableStateFlow<TreeKeys>(emptyMap())
    val keys: StateFlow<TreeKeys> = state.asStateFlow()

    /** Conflated: a screen meeting `noKey` asks the holder for `sessionKeys.mine` now. */
    val missing = Channel<Unit>(Channel.CONFLATED)

    fun of(rootId: String): Map<Int, ByteArray> = state.value[rootId].orEmpty()

    fun merge(more: TreeKeys) {
        if (more.isEmpty()) return
        state.update { current ->
            val next = current.toMutableMap()
            for ((rootId, epochs) in more) next[rootId] = next[rootId].orEmpty() + epochs
            next
        }
    }

    fun drop(rootIds: Set<String>) {
        if (rootIds.isEmpty()) return
        state.update { it - rootIds }
    }

    fun requestMissing() {
        missing.trySend(Unit)
    }
}

/**
 * EXP-1459: one account's tree keys, sealed by the Android Keystore
 * (AES-GCM, like the eNet identity) in app-private storage excluded from
 * backups. One blob per account: `{rootId: {epoch: base64(secret)}}`. A blob
 * that no longer opens is dropped: the holder fetches the wraps again.
 */
class TreeKeyVault(
    private val slots: IdentitySlots,
    private val sealer: SecretSealer,
) {
    private val lock = Any()
    private val cached = mutableMapOf<String, MutableMap<String, Map<Int, ByteArray>>>()

    fun all(accountId: String): TreeKeys = synchronized(lock) { load(accountId).toMap() }

    /** Adds [keys] (merged per root); false when they could not be stored. */
    fun put(accountId: String, keys: TreeKeys): Boolean = synchronized(lock) {
        if (keys.isEmpty()) return true
        val held = load(accountId)
        val next = held.toMutableMap()
        for ((rootId, epochs) in keys) next[rootId] = next[rootId].orEmpty() + epochs
        if (!store(accountId, next)) return false
        held.clear()
        held.putAll(next)
        true
    }

    fun forget(accountId: String) = synchronized(lock) {
        cached.remove(accountId)
        slots.remove(slotKey(accountId))
    }

    private fun load(accountId: String): MutableMap<String, Map<Int, ByteArray>> = cached.getOrPut(accountId) {
        val blob = slots.get(slotKey(accountId))
            ?.let { runCatching { Base64.getDecoder().decode(it) }.getOrNull() }
            ?.let(sealer::open)
        blob?.let { decode(it.toString(Charsets.UTF_8)) }?.toMutableMap() ?: mutableMapOf()
    }

    private fun store(accountId: String, keys: Map<String, Map<Int, ByteArray>>): Boolean {
        val sealed = sealer.seal(encode(keys).toByteArray(Charsets.UTF_8)) ?: return false
        return slots.put(slotKey(accountId), Base64.getEncoder().encodeToString(sealed))
    }

    private fun slotKey(accountId: String) = "trees.$accountId"

    companion object {
        internal fun encode(keys: Map<String, Map<Int, ByteArray>>): String = JsonObject(
            keys.mapValues { (_, epochs) ->
                JsonObject(epochs.entries.associate { (epoch, secret) -> epoch.toString() to JsonPrimitive(Base64.getEncoder().encodeToString(secret)) })
            },
        ).toString()

        internal fun decode(text: String): Map<String, Map<Int, ByteArray>>? = runCatching {
            Json.parseToJsonElement(text).jsonObject.mapValues { (_, epochs) ->
                epochs.jsonObject.entries.associate { (epoch, secret) -> epoch.toInt() to Base64.getDecoder().decode(secret.jsonPrimitive.content) }
                    .filterValues { it.size == TREE_SECRET_BYTES }
            }
        }.getOrNull()

        const val TREE_SECRET_BYTES = 32
    }
}

@Singleton
class TreeKeyStore @Inject constructor(@ApplicationContext context: Context) {
    private val prefs = context.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    val vault = TreeKeyVault(
        slots = object : IdentitySlots {
            override fun get(key: String) = prefs.getString(key, null)
            override fun put(key: String, value: String) = prefs.edit().putString(key, value).commit()
            override fun remove(key: String) {
                prefs.edit().remove(key).commit()
            }
        },
        sealer = KeystoreSealer(KEY_ALIAS),
    )

    companion object {
        /** Excluded from cloud backup + device transfer (res/xml/data_extraction_rules.xml). */
        const val PREFS_FILE = "exp_tree_keys"
        const val KEY_ALIAS = "exp_tree_keys"
    }
}
