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
            // The local half of Settings' "Remove server": wipe the per-account
            // Room files FIRST, then drop the row. The other two steps of that
            // path (push-token unregister, server-side session revoke) are
            // authed calls a tokenless row cannot make — and it holds no push
            // registration, since registration is gated on a live token.
            // Order matters if the process dies mid-prune: a row whose id is
            // already gone leaves its .db files orphaned forever (nothing
            // sweeps them), while a tokenless row whose DB was deleted is
            // harmless — it reopens empty and is pruned again next launch.
            databaseHolder.deleteFiles(id)
            auth.removeAccount(id)
        }
    }

    private companion object {
        const val TAG = "AccountDeduplicator"
    }
}
