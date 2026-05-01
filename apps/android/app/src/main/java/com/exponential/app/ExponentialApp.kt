package com.exponential.app

import android.app.Application
import coil3.ImageLoader
import coil3.SingletonImageLoader
import com.exponential.app.data.electric.SyncManager
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject

@HiltAndroidApp
class ExponentialApp : Application(), SingletonImageLoader.Factory {
    @Inject lateinit var syncManager: SyncManager
    @Inject lateinit var imageLoader: ImageLoader

    override fun onCreate() {
        super.onCreate()
        syncManager.start()
    }

    override fun newImageLoader(context: coil3.PlatformContext): ImageLoader = imageLoader
}
