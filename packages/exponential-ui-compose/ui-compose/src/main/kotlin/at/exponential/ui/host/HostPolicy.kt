package at.exponential.ui.host

import at.exponential.ui.json.JsonValue

// VAPP-91: the host API's policy values (`catalog/host.json` functions /
// urls / media / sources). The RULES live in the Rust core and are reached
// through the facade (`decideFunction`, `decideUrlJson`, `mediaRequestJson`,
// `parseSourceJson`); these are the Kotlin shapes a host configures them with.

/** A function decision (`catalog/host.json` `functions.decisions`). */
enum class FunctionDecision(val wire: String) {
    Allow("allow"), Ask("ask"), Deny("deny"), NotFound("not_found");

    companion object {
        /** The decision named `wire` (unknown = [Deny]). */
        fun of(wire: String): FunctionDecision = entries.firstOrNull { it.wire == wire } ?: Deny
    }
}

/**
 * The function gate: patterns are exact names or a prefix ending in `*`
 * (`harness.*`). Deny wins, then allow, then ask, then [default].
 */
data class FunctionPolicy(
    val allow: List<String> = emptyList(),
    val ask: List<String> = emptyList(),
    val deny: List<String> = emptyList(),
    /** For a registered name no list matches. */
    val default: FunctionDecision = FunctionDecision.Allow,
) {
    /** The contract's JSON. */
    val json: String
        get() = JsonValue.Obj(
            mapOf(
                "allow" to JsonValue.Arr(allow.map(JsonValue::Str)),
                "ask" to JsonValue.Arr(ask.map(JsonValue::Str)),
                "deny" to JsonValue.Arr(deny.map(JsonValue::Str)),
                "default" to JsonValue.Str(default.wire),
            ),
        ).json
}

/**
 * openUrl / Link: the scheme must be in [schemes]; when [hosts] is set an
 * http(s) url's host must match one (exact or `*.example.com`). Relative
 * urls resolve against [baseUrl] (default: the media options' `baseUrl`).
 */
data class UrlPolicy(
    val schemes: List<String>? = null,
    val hosts: List<String>? = null,
    val baseUrl: String? = null,
) {
    internal fun json(fallbackBase: String?): String {
        val m = LinkedHashMap<String, JsonValue>()
        schemes?.let { m["schemes"] = JsonValue.Arr(it.map(JsonValue::Str)) }
        hosts?.let { m["hosts"] = JsonValue.Arr(it.map(JsonValue::Str)) }
        (baseUrl ?: fallbackBase)?.let { m["baseUrl"] = JsonValue.Str(it) }
        return JsonValue.Obj(m).json
    }
}

/** Headers for every media url that starts with [prefix] (later rules win per header). */
data class MediaRule(val prefix: String, val headers: Map<String, String>)

/** The media loader's options: relative urls resolve against [baseUrl]; [rules] add headers. */
data class MediaOptions(val baseUrl: String? = null, val rules: List<MediaRule> = emptyList()) {
    /** The contract's JSON. */
    val json: String
        get() {
            val m = LinkedHashMap<String, JsonValue>()
            baseUrl?.let { m["baseUrl"] = JsonValue.Str(it) }
            m["rules"] = JsonValue.Arr(
                rules.map { r ->
                    JsonValue.Obj(mapOf("prefix" to JsonValue.Str(r.prefix), "headers" to JsonValue.Obj(r.headers.mapValues { JsonValue.Str(it.value) })))
                },
            )
            return JsonValue.Obj(m).json
        }
}

/** A call to a host function, as the policy hook and the handler see it. */
data class FunctionCallInfo(
    val surfaceId: String,
    val componentId: String,
    val name: String,
    val args: Map<String, JsonValue>,
)

/** A registered host function: side effects, may suspend. The result is informational. */
typealias HostFunction = suspend (args: Map<String, JsonValue>, call: FunctionCallInfo) -> Any?

/** What [ExponentialHost.callFunction] did. */
data class FunctionOutcome(val decision: FunctionDecision, val result: Any? = null, val error: String? = null)

/**
 * The host's policy hooks. [onFunctionCall] = the consent hook for `ask`
 * decisions (unset = `ask` is denied); [openUrl] opens an allowed url
 * (default: the system opener through [at.exponential.ui.ExponentialUi.appContext]).
 */
data class HostPolicy(
    val functions: FunctionPolicy? = null,
    val onFunctionCall: (suspend (FunctionCallInfo) -> Boolean)? = null,
    val urls: UrlPolicy? = null,
    val openUrl: ((String) -> Unit)? = null,
    val media: MediaOptions? = null,
)

/** A binding source URI, parsed (`exp:issues?board=b1` → scheme `exp`, name `issues`, params). */
data class ParsedSource(val uri: String, val scheme: String, val name: String, val params: Map<String, String>) {
    companion object {
        /** Parse through the core's rule; null when it does not parse. */
        fun parse(uri: String): ParsedSource? {
            val json = at.exponential.ui.ffi.parseSourceJson(uri) ?: return null
            val v = JsonValue.parse(json)
            return ParsedSource(
                uri = v["uri"]?.string ?: uri,
                scheme = v["scheme"]?.string ?: return null,
                name = v["name"]?.string ?: "",
                params = v["params"]?.obj?.mapValues { it.value.string ?: it.value.json } ?: emptyMap(),
            )
        }
    }
}

/**
 * One resolver per source scheme: subscribe the source; every `emit`
 * writes the value at the bound path (null removes it). Returns the cancel
 * (called when the surface goes away), or null.
 */
fun interface SourceResolver {
    fun subscribe(source: ParsedSource, emit: (JsonValue?) -> Unit): (() -> Unit)?
}
