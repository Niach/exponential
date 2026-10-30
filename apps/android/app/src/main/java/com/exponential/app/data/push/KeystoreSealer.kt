package com.exponential.app.data.push

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Log
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Seals small secrets for app-private storage; null = unusable (no key, tampered, no Keystore). */
interface SecretSealer {
    fun seal(plain: ByteArray): ByteArray?
    fun open(blob: ByteArray): ByteArray?
}

/**
 * AES-256-GCM under a key that lives in the Android Keystore ([alias], never
 * exportable): `blob` = 12-byte IV + ciphertext. A blob restored onto another
 * device (or after the key was wiped) no longer opens, so its owner
 * regenerates the secret instead of trusting a copy.
 */
class KeystoreSealer(private val alias: String) : SecretSealer {
    override fun seal(plain: ByteArray): ByteArray? = try {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, existingKey() ?: createKey())
        val iv = cipher.iv
        check(iv.size == IV_BYTES)
        iv + cipher.doFinal(plain)
    } catch (err: Exception) {
        Log.w(TAG, "seal failed: ${err.javaClass.simpleName}")
        null
    }

    override fun open(blob: ByteArray): ByteArray? = try {
        val key = existingKey()
        if (key == null || blob.size <= IV_BYTES) {
            null
        } else {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, blob, 0, IV_BYTES))
            cipher.doFinal(blob, IV_BYTES, blob.size - IV_BYTES)
        }
    } catch (err: Exception) {
        Log.w(TAG, "open failed: ${err.javaClass.simpleName}")
        null
    }

    private fun existingKey(): SecretKey? {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        return (keyStore.getEntry(alias, null) as? KeyStore.SecretKeyEntry)?.secretKey
    }

    private fun createKey(): SecretKey {
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val TAG = "KeystoreSealer"
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
    }
}
