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

    /** The app's user-facing version (e.g. "0.13.2", "0.13.2-staging"). */
    val VERSION_NAME: String = BuildConfig.VERSION_NAME

    /**
     * Value of the `x-client-version` header sent on every request — the client
     * versioning + min-version gate contract (EXP-104). The server matches on
     * `android/<versionName>` and tolerates the `-staging` suffix.
     */
    val CLIENT_VERSION_HEADER_VALUE: String = "android/${BuildConfig.VERSION_NAME}"
}
