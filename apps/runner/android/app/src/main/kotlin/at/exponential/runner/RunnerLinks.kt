package at.exponential.runner

import at.exponential.ui.json.JsonValue
import java.net.URI
import java.net.URLDecoder

/**
 * lifecycle.json `deepLink`: a standalone runner's links are
 * `<runner.json id>://<screen>?<params>` (the manifest's VIEW filter, scheme
 * patched by E7). The Engine decides what it may open (`crossAppTarget`);
 * this only parses.
 */
object RunnerLinks {
    /** The `deepLink` event JSON for [uri], or null when it is not this runner's link. */
    fun event(uri: String, configId: String): String? {
        val parsed = runCatching { URI(uri) }.getOrNull() ?: return null
        if (parsed.isOpaque || !parsed.scheme.equals(configId, ignoreCase = true)) return null
        val screen = (parsed.rawAuthority ?: parsed.rawPath?.trimStart('/') ?: "").let(::decode)
        val params = LinkedHashMap<String, JsonValue>()
        parsed.rawQuery?.split('&')?.filter { it.isNotEmpty() }?.forEach { pair ->
            val k = pair.substringBefore('=')
            params[decode(k)] = JsonValue.Str(decode(if ('=' in pair) pair.substringAfter('=') else ""))
        }
        return JsonValue.Obj(
            mapOf("type" to JsonValue.Str("deepLink"), "screen" to JsonValue.Str(screen), "params" to JsonValue.Obj(params)),
        ).json
    }

    private fun decode(s: String): String = URLDecoder.decode(s, "UTF-8")
}
