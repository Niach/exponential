package com.exponential.app

/**
 * App-wide constants. The cloud server default and the staging flag come from
 * per-flavor [BuildConfig] fields (see `productFlavors` in app/build.gradle.kts),
 * mirroring iOS `AppConstants.defaultCloudUrl` / `isStaging`. Production builds
 * default to app.exponential.at; staging builds default to next.exponential.at.
 * The multi-server model is unchanged — users can still add any self-hosted URL.
 */
object AppConstants {
    val PUBLIC_CLOUD_URL: String = BuildConfig.DEFAULT_CLOUD_URL
    val IS_STAGING: Boolean = BuildConfig.IS_STAGING
}
