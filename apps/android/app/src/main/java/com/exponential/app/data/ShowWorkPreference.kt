package com.exponential.app.data

import com.exponential.app.data.auth.SecureStore
import com.exponential.app.domain.SHOW_WORK_DEFAULT
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged

/**
 * EXP-1175: the viewer's per-user Show work preference — whether the Run face
 * opens on the full transcript instead of the THREAD (default
 * [SHOW_WORK_DEFAULT], fixture `run-row.json`). Persisted per account (the
 * account = one user on one server), like [TeamSelection]'s last board;
 * SecureStore is not observable, so [version] lets readers re-read.
 */
@Singleton
class ShowWorkPreference @Inject constructor(
    private val secureStore: SecureStore,
) {
    private val _version = MutableStateFlow(0)
    val version: StateFlow<Int> = _version.asStateFlow()

    fun read(accountId: String?): Boolean {
        if (accountId.isNullOrBlank()) return SHOW_WORK_DEFAULT
        return secureStore.get(key(accountId))?.toBooleanStrictOrNull() ?: SHOW_WORK_DEFAULT
    }

    fun observe(accountId: Flow<String?>): Flow<Boolean> =
        combine(accountId, _version) { id, _ -> read(id) }.distinctUntilChanged()

    fun set(accountId: String?, on: Boolean) {
        if (accountId.isNullOrBlank()) return
        secureStore.set(key(accountId), on.toString())
        _version.value += 1
    }

    private fun key(accountId: String) = "run_show_work_$accountId"
}
