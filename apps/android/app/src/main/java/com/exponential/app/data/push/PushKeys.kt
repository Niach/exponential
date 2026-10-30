package com.exponential.app.data.push

import android.content.Context
import android.util.Base64 as AndroidBase64
import android.util.Log
import at.exponential.net.ffi.pushKeypairGenerate
import at.exponential.net.ffi.pushOpen
import dagger.hilt.android.qualifiers.ApplicationContext
import java.util.Base64
import javax.inject.Inject
import javax.inject.Singleton

/** This install's X25519 push keypair (EXP-1435): sealed pushes are opened with [secret]. */
class PushKeys(val secret: ByteArray, val public: ByteArray) {
    /** What `pushTokens.register` takes: base64url, no padding, of the 32 public bytes. */
    val publicBase64Url: String get() = base64UrlEncode(public)
}

internal fun base64UrlEncode(bytes: ByteArray): String =
    Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)

/** Base64url with or without padding; null when [value] is not base64url. */
internal fun base64UrlDecode(value: String): ByteArray? =
    runCatching { Base64.getUrlDecoder().decode(value.trim()) }.getOrNull()

/**
 * The eNet push-only facade (`exponential-net-ffi --no-default-features`,
 * the `.so` under apps/desktop/crates/exponential-net-ffi/out/push/jniLibs).
 * Every call is guarded: a build without the native library (a local build
 * that skipped the Rust step) throws UnsatisfiedLinkError / NoClassDefFoundError
 * from the binding's loader, and that must degrade to the bare-token
 * registration, never crash the app or the FCM service.
 */
internal object PushCrypto {
    private const val TAG = "PushCrypto"

    fun generate(): PushKeys? = guarded("generate") {
        val pair = pushKeypairGenerate()
        PushKeys(secret = pair.secret, public = pair.public)
    }

    /** The plaintext, or null: unopenable (wrong key, tampered, unknown version) or no facade. */
    fun open(secret: ByteArray, sealed: ByteArray): ByteArray? = guarded("open") { pushOpen(secret, sealed) }

    private inline fun <T> guarded(op: String, block: () -> T): T? = try {
        block()
    } catch (err: Throwable) {
        // The class only: an EnetException's reason never carries key or
        // payload bytes, but nothing about a push's content belongs in logcat.
        Log.w(TAG, "push $op failed: ${err.javaClass.simpleName}")
        null
    }
}

/**
 * Keeps the push keypair in app-private storage: the public key as is, the
 * 32-byte secret encrypted under an AES-256-GCM key that lives in the Android
 * Keystore (alias [KEY_ALIAS], never exportable). Generated once, on the first
 * registration that has a relay to register with; [secret] never generates, so
 * the FCM service only ever opens with the key the servers were given.
 *
 * A secret that no longer decrypts (the Keystore key is gone, e.g. the prefs
 * came back from a backup onto another device) is replaced by a fresh pair;
 * the reconcile pass re-registers every account at each cold start, so the
 * servers learn the new public key.
 */
@Singleton
class PushKeyStore @Inject constructor(@ApplicationContext context: Context) {
    private val prefs = context.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)
    private val sealer = KeystoreSealer(KEY_ALIAS)
    private val lock = Any()
    private var cached: PushKeys? = null

    /** The keypair, generated on first use; null when the facade or the Keystore is unavailable. */
    fun keypair(): PushKeys? = synchronized(lock) {
        load() ?: PushCrypto.generate()?.takeIf { store(it) }
    }

    /** The secret to open a sealed push with, or null when none was ever generated (or it is unreadable). */
    fun secret(): ByteArray? = synchronized(lock) { load()?.secret }

    private fun load(): PushKeys? {
        cached?.let { return it }
        val public = prefs.getString(KEY_PUBLIC, null) ?: return null
        val sealed = prefs.getString(KEY_SECRET, null) ?: return null
        val blob = runCatching { AndroidBase64.decode(sealed, AndroidBase64.NO_WRAP) }.getOrNull()
        val secret = blob?.let(sealer::open) ?: run {
            Log.w(TAG, "push key unreadable")
            return null
        }
        return PushKeys(secret, AndroidBase64.decode(public, AndroidBase64.NO_WRAP)).also { cached = it }
    }

    private fun store(keys: PushKeys): Boolean {
        // Not persisted = not usable: handing a server a public key whose
        // secret is gone after a restart would make every push unopenable.
        val blob = sealer.seal(keys.secret) ?: return false
        val ok = prefs.edit()
            .putString(KEY_PUBLIC, AndroidBase64.encodeToString(keys.public, AndroidBase64.NO_WRAP))
            .putString(KEY_SECRET, AndroidBase64.encodeToString(blob, AndroidBase64.NO_WRAP))
            .commit()
        if (ok) cached = keys
        return ok
    }

    companion object {
        private const val TAG = "PushKeyStore"
        private const val PREFS_FILE = "exp_push_key"
        private const val KEY_PUBLIC = "x25519_public"
        private const val KEY_SECRET = "x25519_secret_sealed"
        const val KEY_ALIAS = "exp_push_x25519"
    }
}
