package com.exponential.app.domain

/**
 * #VAPP-229 (fixtures/via-app.json): the "via <app>" caption of every row an
 * app caused (issues, sessions, records; comments read reporter-reply.json's
 * `captions.viaApp`, the same string). Mirrors the TS reference
 * `packages/domain-contract/src/via-app.ts`; [ViaAppTest] replays the cases.
 */
object ViaApp {
    const val COPY = "via {app}"
    const val SEPARATOR = " · "
}

/** "via {app}", or null without a (resolved, non-empty) app name. */
fun viaAppCaption(appName: String?): String? =
    if (appName.isNullOrEmpty()) null else ViaApp.COPY.replace("{app}", appName)

/** [text] with the caption appended after the separator (the text alone
 *  without an app). */
fun withViaApp(text: String, appName: String?): String {
    val caption = viaAppCaption(appName) ?: return text
    return "$text${ViaApp.SEPARATOR}$caption"
}
