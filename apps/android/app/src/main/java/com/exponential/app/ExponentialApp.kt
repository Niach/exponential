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
import com.exponential.app.data.auth.TimezoneClaimer
import com.exponential.app.data.auth.legacyDbIdToWipe
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import com.exponential.app.data.steer.SteerConnectionStore
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject

private const val KEY_PERUSER_DB_CLEANUP = "peruser_db_cleanup_v1"

@HiltAndroidApp
class ExponentialApp : Application(), SingletonImageLoader.Factory {
    @Inject lateinit var syncManager: SyncManager
    @Inject lateinit var pushTokenManager: PushTokenManager
    @Inject lateinit var timezoneClaimer: TimezoneClaimer
    @Inject lateinit var imageLoader: ImageLoader
    @Inject lateinit var auth: AuthRepository
    @Inject lateinit var databaseHolder: DatabaseHolder
    @Inject lateinit var secureStore: SecureStore
    @Inject lateinit var accountDeduplicator: AccountDeduplicator
    @Inject lateinit var steerConnectionStore: SteerConnectionStore

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
        // Claim-once loop: stamps the device timezone on accounts that never
        // had one captured (EXP-452 — mobile-only accounts digested in UTC).
        timezoneClaimer.start()
        // The shape loops only run while the app is visible (REV2-38): the gate
        // starts closed, ON_START opens it and ON_STOP closes it AT ONCE
        // (EXP-656 — the old 30s grace timer was a coroutine delay on a clock
        // that stops in deep sleep, so a phone sleeping right after ON_STOP
        // regularly carried 19 open long-polls into suspension). A
        // backgrounded (or push-woken) process therefore holds no shape
        // connections at all.
        //
        // Nothing is kicked here on the way back. `setForeground(true)` drops
        // the idle pooled connections a sleeping radio may have killed and
        // then reopens the gate, and the gate's edge is itself the kick every
        // shape loop needs (ShapeClient.run) — a kick fan-out on top would
        // cancel the very catch-up polls the gate just started, which is what
        // the ~10s of stale content on open (EXP-264) had degenerated into.
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                // Order matters and must stay this way (EXP-656): setForeground
                // stamps lastKickAt, which the steer store fans out to its
                // connections, and setForeground(true) below then calls
                // resume() on each of them. Adding a kick of either kind here
                // buys nothing and costs a second redial per open session.
                syncManager.setForeground(true)
                // Same gate for the live steer sockets (EXP-621): they are
                // app-held now, so backgrounding parks them after their own
                // grace window and coming back revives them.
                steerConnectionStore.setForeground(true)
            }

            override fun onStop(owner: LifecycleOwner) {
                syncManager.setForeground(false)
                steerConnectionStore.setForeground(false)
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
