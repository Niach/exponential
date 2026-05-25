package com.exponential.app

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import coil3.ImageLoader
import coil3.SingletonImageLoader
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject

@HiltAndroidApp
class ExponentialApp : Application(), SingletonImageLoader.Factory {
    @Inject lateinit var syncManager: SyncManager
    @Inject lateinit var pushTokenManager: PushTokenManager
    @Inject lateinit var imageLoader: ImageLoader
    @Inject lateinit var auth: AuthRepository
    @Inject lateinit var databaseHolder: DatabaseHolder

    override fun onCreate() {
        super.onCreate()
        createIssueNotificationChannel()
        // Open a Room instance for every signed-in account up front so the
        // DAO facades' `holder.database` StateFlow has something to emit
        // before SyncManager's first reconcile tick lands. Without this,
        // HomeViewModel.bootstrap() can race the first pipeline launch and
        // throw on `holder.current()`. Open non-active first then active
        // last so the active account is what the transitional `current()`
        // resolves to.
        val activeId = auth.activeAccountId.value
        val accounts = auth.accounts.value
        for (account in accounts) {
            if (account.id == activeId) continue
            if (account.token != null) {
                databaseHolder.database(forAccountId = account.id)
            }
        }
        activeId?.let { id ->
            if (accounts.any { it.id == id && it.token != null }) {
                databaseHolder.database(forAccountId = id)
            }
        }
        syncManager.start()
        pushTokenManager.start()
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
