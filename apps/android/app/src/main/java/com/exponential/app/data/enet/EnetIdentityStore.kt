package com.exponential.app.data.enet

import android.content.Context
import com.exponential.app.data.push.KeystoreSealer
import com.exponential.app.data.push.SecretSealer
import dagger.hilt.android.qualifiers.ApplicationContext
import java.util.Base64
import javax.inject.Inject
import javax.inject.Singleton

/** The few string slots [EnetIdentityVault] persists in (SharedPreferences in the app, a map in tests). */
interface IdentitySlots {
    fun get(key: String): String?
    fun put(key: String, value: String): Boolean
    fun remove(key: String)
}

/**
 * EXP-1457 / VAPP-37: this install's eNet identity, one per signed-in
 * account (the server binds an endpoint id to exactly one user). 32 ed25519
 * secret bytes, generated once, kept ONLY sealed by [sealer] (the Android
 * Keystore) in app-private storage that is excluded from backups: never
 * synced, never uploaded. A slot that no longer opens (the Keystore key is
 * gone) is replaced by a fresh identity, which simply registers as a new
 * install.
 */
class EnetIdentityVault(
    private val slots: IdentitySlots,
    private val sealer: SecretSealer,
    private val generate: () -> ByteArray,
) {
    private val lock = Any()
    private val cached = mutableMapOf<String, ByteArray>()

    /**
     * The account's identity, generated and stored on first use ([create]);
     * null when it cannot be stored (or none exists and [create] is false).
     */
    fun identity(accountId: String, create: Boolean = true): ByteArray? = synchronized(lock) {
        cached[accountId]?.let { return it }
        val key = slotKey(accountId)
        val stored = slots.get(key)
            ?.let { runCatching { Base64.getDecoder().decode(it) }.getOrNull() }
            ?.let(sealer::open)
            ?.takeIf { it.size == IDENTITY_BYTES }
        val identity = stored ?: run {
            if (!create) return null
            val fresh = generate()
            require(fresh.size == IDENTITY_BYTES)
            // Not persisted = not usable: a node whose identity is gone after a
            // restart would register a new install every launch.
            val sealed = sealer.seal(fresh) ?: return null
            if (!slots.put(key, Base64.getEncoder().encodeToString(sealed))) return null
            fresh
        }
        cached[accountId] = identity
        identity
    }

    /** Drops the account's identity (explicit sign-out, or the server revoked it). */
    fun forget(accountId: String) = synchronized(lock) {
        cached.remove(accountId)
        slots.remove(slotKey(accountId))
    }

    private fun slotKey(accountId: String) = "identity.$accountId"

    companion object {
        const val IDENTITY_BYTES = 32
    }
}

@Singleton
class EnetIdentityStore @Inject constructor(@ApplicationContext context: Context) {
    private val prefs = context.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    val vault = EnetIdentityVault(
        slots = object : IdentitySlots {
            override fun get(key: String) = prefs.getString(key, null)
            override fun put(key: String, value: String) = prefs.edit().putString(key, value).commit()
            override fun remove(key: String) {
                prefs.edit().remove(key).commit()
            }
        },
        sealer = KeystoreSealer(KEY_ALIAS),
        // Off the main thread only: the first call loads JNA + the eNet `.so`.
        generate = { EnetFfi.generateIdentity() },
    )

    companion object {
        /** Excluded from cloud backup + device transfer (res/xml/data_extraction_rules.xml). */
        const val PREFS_FILE = "exp_enet_identity"
        const val KEY_ALIAS = "exp_enet_identity"
    }
}
