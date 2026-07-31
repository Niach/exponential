package com.exponential.app

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import coil3.ImageLoader
import coil3.SingletonImageLoader
import com.exponential.app.data.auth.AccountDeduplicator
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
    @Inject lateinit var accountDeduplicator: AccountDeduplicator

    override fun onCreate() {
        super.onCreate()
        createIssueNotificationChannel()
        cleanupLegacyAccountDatabases()
        // Self-heal a device that already carries a duplicate server row, before
        // anything opens its DB or starts its pipelines.
        accountDeduplicator.prune()
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
        // The shape loops only run while the app is visible (REV2-38): the gate
        // starts closed, ON_START opens it and ON_STOP parks the loops after a
        // grace window, so a backgrounded (or push-woken) process holds no
        // shape connections.
        //
        // Coming back to the foreground, the loops may also be sitting in a
        // stale backoff or holding a socket the radio killed while we were
        // away — that was the ~10s of stale content on open (EXP-264). Kick
        // them so the first thing the user sees is current. On a cold launch
        // this fires immediately after start(), where the freshness window
        // makes it a harmless no-op.
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                syncManager.setForeground(true)
                syncManager.kick("app-foreground")
            }

            override fun onStop(owner: LifecycleOwner) {
                syncManager.setForeground(false)
            }
        })
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
