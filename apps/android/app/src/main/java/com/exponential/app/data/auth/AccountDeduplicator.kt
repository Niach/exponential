package com.exponential.app.data.auth

import android.util.Log
import com.exponential.app.data.db.DatabaseHolder
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Keeps the servers list at one row per instance (iOS parity): a signed-out row
 * shadowed by a signed-in row on the same server is removed. See
 * [duplicateSignedOutAccountIds] for what is (and is never) removable.
 *
 * Runs at two points:
 *  - right after a login persists an account, which is where the duplicate is
 *    born (a re-signup mints a new userId, hence a new per-user row), and
 *  - at startup, so devices already carrying a duplicate heal on update.
 */
@Singleton
class AccountDeduplicator @Inject constructor(
    private val auth: AuthRepository,
    private val databaseHolder: DatabaseHolder,
) {

    fun prune() {
        val ids = duplicateSignedOutAccountIds(auth.accounts.value, auth.activeAccountId.value)
        for (id in ids) {
            Log.i(TAG, "removing signed-out duplicate account $id")
            // The local half of Settings' "Remove server": drop the row, then
            // wipe its per-account Room files. The other two steps of that path
            // (push-token unregister, server-side session revoke) are authed
            // calls a tokenless row cannot make — and it holds no push
            // registration, since registration is gated on a live token.
            auth.removeAccount(id)
            databaseHolder.deleteFiles(id)
        }
    }

    private companion object {
        const val TAG = "AccountDeduplicator"
    }
}
