package at.exponential.runner

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import at.exponential.engine.ffi.Vault
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * runner.json `storage`: the Vault = Android Keystore-sealed preferences.
 * Each value is AES-GCM sealed under a non-exportable Keystore key
 * (`iv ‖ ciphertext`, base64) in a private SharedPreferences file; the key
 * name is the preference key, so nothing readable leaves the Keystore.
 * Grants, the last screen and the `local` device uuid live here.
 */
class KeystoreVault(context: Context) : Vault {
    private val prefs = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    override fun `read`(`key`: String): String? {
        val sealed = prefs.getString(key, null) ?: return null
        return runCatching {
            val bytes = Base64.decode(sealed, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORM)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, IV))
            String(cipher.doFinal(bytes, IV, bytes.size - IV), Charsets.UTF_8)
        }.getOrNull()
    }

    override fun `write`(`key`: String, `value`: String) {
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val sealed = cipher.iv + cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        prefs.edit().putString(key, Base64.encodeToString(sealed, Base64.NO_WRAP)).apply()
    }

    override fun `delete`(`key`: String) {
        prefs.edit().remove(key).apply()
    }

    private fun key(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val FILE = "exponential.runner.vault"
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "exponential.runner.vault"
        const val TRANSFORM = "AES/GCM/NoPadding"
        const val IV = 12
    }
}
