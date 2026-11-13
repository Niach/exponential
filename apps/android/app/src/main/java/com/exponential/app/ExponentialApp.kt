package com.exponential.app

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.lifecycle.lifecycleScope
import coil3.ImageLoader
import coil3.SingletonImageLoader
import com.exponential.app.data.auth.AccountDeduplicator
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.SecureStore
import com.exponential.app.data.auth.TimezoneClaimer
import com.exponential.app.data.auth.legacyDbIdToWipe
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.push.PushTokenManager
import com.exponential.app.data.steer.SteerConnectionStore
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import io.ktor.client.HttpClient

private const val KEY_PERUSER_DB_CLEANUP = "peruser_db_cleanup_v1"

@HiltAndroidApp
class ExponentialApp : Application(), SingletonImageLoader.Factory {
    init {
        // #PERF-11: before any binding loads (MobileNativeLibrary).
        MobileNativeLibrary.install()
    }

    @Inject lateinit var syncManager: SyncManager
    @Inject lateinit var composerDrafts: com.exponential.app.data.composer.ComposerDrafts
    @Inject lateinit var appRecordOutbox: com.exponential.app.ui.apps.AppRecordOutbox
    @Inject lateinit var sessionOutbox: com.exponential.app.data.outbox.SessionOutbox
    @Inject lateinit var pushTokenManager: PushTokenManager
    @Inject lateinit var enetNodeManager: EnetNodeManager
    @Inject lateinit var timezoneClaimer: TimezoneClaimer
    @Inject lateinit var imageLoader: ImageLoader
    @Inject lateinit var auth: AuthRepository
    @Inject lateinit var httpClient: HttpClient
    @Inject lateinit var databaseHolder: DatabaseHolder
    @Inject lateinit var secureStore: SecureStore
    @Inject lateinit var accountDeduplicator: AccountDeduplicator
    @Inject lateinit var steerConnectionStore: SteerConnectionStore
    @Inject lateinit var directShareShortcuts: com.exponential.app.data.share.DirectShareShortcuts
    @Inject lateinit var systemSearchIndexer: com.exponential.app.data.search.SystemSearchIndexer

    override fun onCreate() {
        // #LOGS-2: before Hilt injects, so a crash building the graph is reported.
        Observability.start(this)
        super.onCreate()
        // #EXP-1629: a genUI theme's `JetBrains Mono` = the bundled face.
        at.exponential.ui.ExponentialUi.registerFont(com.exponential.app.ui.theme.EXP_MONO_FAMILY, com.exponential.app.ui.theme.ExpMono)
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
        // #LOGS-2: the active server's monitoring config + user id.
        ProcessLifecycleOwner.get().lifecycleScope.launch {
            auth.userId.collect(Observability::setUser)
        }
        // No server chosen yet (a fresh install on the instance picker) = the
        // build's own cloud, so a crash before sign-in is reported too.
        ProcessLifecycleOwner.get().lifecycleScope.launch(Dispatchers.IO) {
            auth.instanceUrl.collectLatest { Observability.refresh(httpClient, it ?: AppConstants.PUBLIC_CLOUD_URL) }
        }
        // EXP-1635: the system share sheet's rows follow the synced agents + chats.
        directShareShortcuts.start()
        // #EXP-1642: issues and chats in the phone's own search (AppSearch).
        systemSearchIndexer.start()
        // VAPP-235: replay the offline outbox at launch and whenever sync comes back.
        appRecordOutbox.start()
        // #FEED-237: resend the session messages the server never answered.
        sessionOutbox.start()
        pushTokenManager.start()
        // EXP-1457: one eNet node per signed-in account, started lazily on
        // Dispatchers.IO (JNA + the .so never load on the main thread).
        enetNodeManager.start()
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
                // EXP-1531: the network may have changed while we were away:
                // the eNet nodes re-probe and migrate their paths.
                enetNodeManager.networkChange("foreground")
                // Same gate for the live steer sockets (EXP-621): they are
                // app-held now, so backgrounding parks them after their own
                // grace window and coming back revives them.
                steerConnectionStore.setForeground(true)
            }

            override fun onStop(owner: LifecycleOwner) {
                // #FEED-219: the app hiding writes every pending composer
                // draft now (web's `pagehide`).
                composerDrafts.flushAll()
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
