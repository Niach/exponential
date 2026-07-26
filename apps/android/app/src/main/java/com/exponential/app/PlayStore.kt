package com.exponential.app

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri

/**
 * Google Play listing links — the store surface shared by the "Rate our app"
 * settings row (EXP-260) and the 426 update gate's fallback.
 *
 * Android is the only client with a published store listing so far, which is
 * why there is no iOS/App Store counterpart yet.
 */
object PlayStore {
    /**
     * The package id to link to. The staging flavor installs as
     * `at.exponential.staging`, which has no listing of its own — strip the
     * suffix so its links land on the published production app.
     */
    fun listingAppId(packageName: String): String = packageName.removeSuffix(".staging")

    /** Play app deep link — opens the listing (and its rating widget) natively. */
    fun marketUri(appId: String): String = "market://details?id=$appId"

    /** Browser fallback for devices without the Play Store app. */
    fun webUrl(appId: String): String =
        "https://play.google.com/store/apps/details?id=$appId"

    /**
     * Open this app's Play listing: the Play app first, the web listing when
     * Play is absent. Returns false when neither could be launched (no Play,
     * no browser), so callers can surface that instead of appearing to no-op.
     */
    fun openListing(context: Context): Boolean {
        val appId = listingAppId(context.packageName)
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(marketUri(appId))))
            return true
        } catch (_: ActivityNotFoundException) {
            // Fall through to the browser.
        }
        return runCatching {
            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(webUrl(appId))))
        }.isSuccess
    }
}
