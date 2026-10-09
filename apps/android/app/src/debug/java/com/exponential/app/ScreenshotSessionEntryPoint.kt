package com.exponential.app

import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent

/**
 * CAPTURE-ONLY seam for the screenshot suites (EXP-1267), DEBUG builds only
 * (src/debug — no release build contains it).
 *
 * The suites used to reinstall the app before every run and type the demo
 * credentials into the instance picker and the login form. The instrumentation
 * test runs in the app's own process, so it can instead sign in through the
 * app's REAL login path (`AuthApi.signInWithPassword` / `completeLogin`, the
 * same calls LoginViewModel makes) before the activity launches — and reuse
 * that session across runs. This entry point is how the test reaches the app's
 * Hilt singletons; see androidTest `ScreenshotSession`.
 *
 * Nothing in the product reads it.
 */
@EntryPoint
@InstallIn(SingletonComponent::class)
interface ScreenshotSessionEntryPoint {
    fun authRepository(): AuthRepository
    fun authApi(): AuthApi
}
