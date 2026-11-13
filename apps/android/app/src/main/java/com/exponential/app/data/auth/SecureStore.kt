package com.exponential.app.data.auth

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

private const val PREFS_FILE = "exponential_secure_prefs"

@Singleton
open class SecureStore internal constructor(
    private val prefsProvider: () -> SharedPreferences,
) {
    @Inject constructor(@ApplicationContext context: Context) : this({ securePrefs(context) })

    private val prefs: SharedPreferences by lazy(prefsProvider)

    open fun get(key: String): String? = prefs.getString(key, null)

    open fun set(key: String, value: String?) {
        prefs.edit().apply {
            if (value == null) remove(key) else putString(key, value)
            apply()
        }
    }

    /** Several keys in ONE edit, written through synchronously (`commit()`):
     *  the account set and its active id must never land half-written. */
    open fun setAll(values: Map<String, String?>) {
        val editor = prefs.edit()
        for ((key, value) in values) if (value == null) editor.remove(key) else editor.putString(key, value)
        editor.commit()
    }
}

private fun securePrefs(context: Context): SharedPreferences = run {
    val masterKey = MasterKey.Builder(context)
        .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
        .build()

    EncryptedSharedPreferences.create(
        context,
        PREFS_FILE,
        masterKey,
        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
    )
}
