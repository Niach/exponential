package com.exponential.app

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import coil3.ImageLoader
import coil3.SingletonImageLoader
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject

@HiltAndroidApp
class ExponentialApp : Application(), SingletonImageLoader.Factory {
    @Inject lateinit var syncManager: SyncManager
    @Inject lateinit var pushTokenManager: PushTokenManager
    @Inject lateinit var imageLoader: ImageLoader

    override fun onCreate() {
        super.onCreate()
        createIssueNotificationChannel()
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
