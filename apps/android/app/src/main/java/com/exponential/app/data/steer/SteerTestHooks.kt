package com.exponential.app.data.steer

import androidx.annotation.VisibleForTesting
import java.net.URI

/**
 * CAPTURE-ONLY seam for the screenshot suites (EXP-812).
 *
 * The relay URL is never assembled on the client: the server mints the whole
 * `ws(s)://<relay>/ws?ticket=…` dial URL from its own STEER_RELAY_URL and the
 * app dials it verbatim. Inside an emulator that breaks down — the host's
 * `localhost` is the emulator itself — so the capture run used to have to
 * restart the WEB SERVER on the host's LAN IP, which then blocked the browser
 * lanes (Chromium treats `ws://192.168.x.x` from an https page as mixed
 * content) and forced android into a pass of its own.
 *
 * Rewriting the AUTHORITY on the client instead lets one server serve every
 * lane: it keeps `ws://localhost:4002`, which it can reach for its own device
 * presence, while the emulator dials `ws://10.0.2.2:4002`.
 *
 * Nothing in the product reads or writes this — no route, no setting, no deep
 * link. `StoreScreenshotsTest` sets it from the `steerRelayUrl` instrumentation
 * argument, and only when the instance it signs into is host-local; it stays
 * null in every real run, and [KtorSteerTransport] only consults it on a debug
 * build anyway.
 */
@VisibleForTesting
object SteerTestHooks {
    /** `ws://10.0.2.2:4002` — scheme, host and port only; null = dial as minted. */
    @Volatile
    var relayAuthority: String? = null
}

/**
 * [url] with its scheme, host and port replaced by [authority]'s, keeping the
 * path and the query — the ticket lives in the query and must survive.
 * Anything unparsable returns [url] unchanged: a capture-only rewrite must
 * never be the reason a dial fails.
 */
internal fun withRelayAuthority(url: String, authority: String): String =
    try {
        val target = URI(authority)
        val original = URI(url)
        require(!target.scheme.isNullOrEmpty() && !target.host.isNullOrEmpty())
        val port = if (target.port >= 0) ":${target.port}" else ""
        val query = original.rawQuery?.let { "?$it" } ?: ""
        "${target.scheme}://${target.host}$port${original.rawPath}$query"
    } catch (_: Exception) {
        url
    }
