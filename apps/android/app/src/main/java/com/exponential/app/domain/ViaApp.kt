package com.exponential.app.domain

import kotlinx.serialization.Serializable

/**
 * #VAPP-229 (fixtures/via-app.json): the "via <app>" caption of every row an
 * app caused (issues, sessions, records; comments read reporter-reply.json's
 * `captions.viaApp`, the same string). Mirrors the TS reference
 * `packages/domain-contract/src/via-app.ts`; [ViaAppTest] replays the cases.
 */
object ViaApp {
    const val COPY = "via {app}"
    const val SEPARATOR = " · "
    /** #VAPP-231, fixture `claim.header`: the MCP header a claim rides on. */
    const val HEADER = "X-Exp-Via-App"
}

/**
 * #VAPP-231 (fixture `claim`): a host's claim that a write comes from its app
 * (`viaApp` on a records write); the server checks it, it only adds the caption.
 */
@Serializable
data class ViaAppClaim(val appId: String, val version: Int) {
    /** The MCP header's value, `{appId}@{version}`. */
    val headerValue: String get() = "$appId@$version"
}

/** The claim of a host over [appId]; null without an app or a published version. */
fun viaAppClaim(appId: String?, publishedVersion: Int?): ViaAppClaim? =
    if (appId.isNullOrEmpty() || publishedVersion == null || publishedVersion <= 0) null else ViaAppClaim(appId, publishedVersion)

/** "via {app}", or null without a (resolved, non-empty) app name. */
fun viaAppCaption(appName: String?): String? =
    if (appName.isNullOrEmpty()) null else ViaApp.COPY.replace("{app}", appName)

/** [text] with the caption appended after the separator (the text alone
 *  without an app). */
fun withViaApp(text: String, appName: String?): String {
    val caption = viaAppCaption(appName) ?: return text
    return "$text${ViaApp.SEPARATOR}$caption"
}
