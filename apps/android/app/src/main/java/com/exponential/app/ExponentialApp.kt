package com.exponential.app

import android.app.Application
import com.exponential.app.data.electric.SyncManager
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject

@HiltAndroidApp
class ExponentialApp : Application() {
    @Inject lateinit var syncManager: SyncManager

    override fun onCreate() {
        super.onCreate()
        syncManager.start()
    }
}
