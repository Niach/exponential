package com.exponential.app.domain

import at.exponential.ui.json.JsonValue

/**
 * VAPP-230 (eApps M1): `upstream:<server>/<tool>?<args>` read bindings, the
 * host half (`packages/emcp/fixtures/upstream.json` `binding`: its
 * `$comment` is the spec; TS `@exp/emcp/upstream`, the web's
 * `UpstreamAnswers`). The host asks the server (tRPC `apps.upstreamRead`)
 * and re-reads every [CACHE_SECONDS] while bound; a server answering
 * [DISCONNECTED] raises one "Connect <server>" banner.
 */
object AppUpstream {
    /** The binding scheme (`binding.scheme`). */
    const val SCHEME = "upstream"

    /** A host re-reads a live binding this often (`binding.cacheSeconds`). */
    const val CACHE_SECONDS = 60

    /** The banner text (`binding.banner`), `{server}` = the server. */
    const val BANNER = "Connect {server}"

    /** The error that raises the banner (`errors.disconnected`). */
    const val DISCONNECTED = "upstreamDisconnected"

    /** The banner's action label (opens Settings › MCP; the web's). */
    const val CONNECT = "Connect"

    /** One read's answer: `{ok: true, value}` | `{ok: false, error, server}`. */
    data class Answer(val ok: Boolean, val value: JsonValue? = null, val error: String? = null, val server: String? = null) {
        companion object {
            fun of(json: JsonValue): Answer = if (json["ok"]?.bool == true) {
                Answer(ok = true, value = json["value"]?.takeIf { !it.isNull } ?: JsonValue.Null)
            } else {
                Answer(ok = false, error = json["error"]?.string, server = json["server"]?.string)
            }
        }
    }

    /** One banner: its server and text. */
    data class Banner(val server: String, val text: String)

    /** One banner per server whose read answered [DISCONNECTED], sorted, each once. */
    fun banners(answers: Collection<Answer>): List<Banner> =
        answers.mapNotNullTo(sortedSetOf<String>()) { a -> a.server?.takeIf { !a.ok && a.error == DISCONNECTED } }
            .map { Banner(it, BANNER.replace("{server}", it)) }

    /** The banner texts (`upstreamBanners`). */
    fun upstreamBanners(answers: Collection<Answer>): List<String> = banners(answers).map { it.text }
}
