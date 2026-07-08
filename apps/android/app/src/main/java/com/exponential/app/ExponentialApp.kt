package com.exponential.app

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import coil3.ImageLoader
import coil3.SingletonImageLoader
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.SecureStore
import com.exponential.app.data.auth.legacyDbIdToWipe
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject

private const val KEY_PERUSER_DB_CLEANUP = "peruser_db_cleanup_v1"

@HiltAndroidApp
class ExponentialApp : Application(), SingletonImageLoader.Factory {
    @Inject lateinit var syncManager: SyncManager
    @Inject lateinit var pushTokenManager: PushTokenManager
    @Inject lateinit var imageLoader: ImageLoader
    @Inject lateinit var auth: AuthRepository
    @Inject lateinit var databaseHolder: DatabaseHolder
    @Inject lateinit var secureStore: SecureStore

    override fun onCreate() {
        super.onCreate()
        createIssueNotificationChannel()
        cleanupLegacyAccountDatabases()
        // Open a Room instance for every signed-in account up front so
        // ViewModels that resolve `holder.database(forAccountId:)` at init
        // time get a cached instance instead of racing the first
        // SyncManager reconcile tick.
        for (account in auth.accounts.value) {
            if (account.token != null) {
                databaseHolder.database(forAccountId = account.id)
            }
        }
        syncManager.start()
        pushTokenManager.start()
    }

    // One-shot: AccountStore has re-keyed accounts to per-user ids by now. The
    // old URL-only DB file (`exponential-<urlOnlyId>-v2.db`) may hold a DIFFERENT
    // user's cached data — the "logged into the wrong account" bug — so wipe it
    // rather than rename. Covers both re-keyed accounts AND accounts the
    // migration left tokenless (userId unknown), whose URL-keyed DB is exactly
    // the wrong-user cache. Each affected account resyncs once under its new id.
    private fun cleanupLegacyAccountDatabases() {
        if (secureStore.get(KEY_PERUSER_DB_CLEANUP) == "done") return
        for (account in auth.accounts.value) {
            legacyDbIdToWipe(account)?.let { databaseHolder.deleteFiles(it) }
        }
        secureStore.set(KEY_PERUSER_DB_CLEANUP, "done")
    }

    override fun newImageLoader(context: coil3.PlatformContext): ImageLoader = imageLoader

    private fun createIssueNotificationChannel() {
        val manager = getSystemService(NotificationManager::class.java) ?: return
        val channel = NotificationChannel(
            ISSUE_CHANNEL_ID,
            "Issues",
            NotificationManager.IMPORTANCE_DEFAULT,
        ).apply {
            description = "Notifications about issue assignments and updates"
        }
        manager.createNotificationChannel(channel)
    }

    companion object {
        const val ISSUE_CHANNEL_ID = "issues_default"
    }
}
