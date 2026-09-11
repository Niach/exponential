package com.exponential.app.data.media

import android.content.Context
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.domain.MediaPreparer
import dagger.Binds
import dagger.Module
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent

/** Binds the Media3-backed preparer behind the pure-Kotlin seam (EXP-824). */
@Module
@InstallIn(SingletonComponent::class)
abstract class MediaModule {
    @Binds
    abstract fun bindMediaPreparer(impl: AndroidMediaPreparer): MediaPreparer
}

/**
 * How the markdown composables reach the media singletons without threading
 * them through every screen (the same idiom the emoji sheet and deep links
 * use): the preparer for uploads and the auth repository the player's
 * authenticated data source resolves tokens from.
 */
@EntryPoint
@InstallIn(SingletonComponent::class)
interface MediaEntryPoint {
    fun mediaPreparer(): MediaPreparer
    fun authRepository(): AuthRepository
}

fun mediaEntryPoint(context: Context): MediaEntryPoint =
    EntryPointAccessors.fromApplication(context.applicationContext, MediaEntryPoint::class.java)
