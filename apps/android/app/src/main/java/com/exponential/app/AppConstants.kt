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

    /**
     * The desktop app's releases page — the getting-started checklist's
     * "Download the desktop app" target. Same value as web's
     * `lib/desktop-download.ts` `DESKTOP_RELEASES_URL`; a phone can't pick an
     * OS asset, so it always opens the page.
     */
    const val DESKTOP_RELEASES_URL: String = "https://github.com/Niach/exponential/releases/latest"

    /**
     * The headless-daemon install one-liner for [origin] (web's
     * `buildServerInstallSnippet`). One script for cloud and self-host alike —
     * the target instance always rides `EXP_INSTANCE` explicitly.
     */
    fun serverInstallSnippet(origin: String): String =
        "curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=$origin sh"
}
