package at.exponential.ui.host

import at.exponential.ui.ffi.navigateTargetJson
import at.exponential.ui.ffi.resolveScreenJson
import at.exponential.ui.json.JsonValue

/**
 * VAPP-218 (eApps U1, spec `docs/spec/apps.md` §5.1): a package's SCREENS,
 * the navigation manifest (`@exponential_app/eui` `src/host/screens.ts`,
 * eui-core `host::screens`, `fixtures/host-router.json` `screens` +
 * `navigate`). The HOST draws the navigation from it; the `app.navigate
 * {screen, params?}` host function opens one, its params landing under
 * `/params/` in the template data.
 */
object Screens {
    /** The host function (and action call) that opens a screen; always allowed (the package need not list it). */
    const val NAVIGATE_FUNCTION = "app.navigate"

    /** A screen as a host opens it. */
    data class Resolved(val screen: String, val title: String, val icon: String?, val params: Map<String, String>) {
        /** The template data it opens with: `{params}`. */
        val data: JsonValue get() = JsonValue.Obj(mapOf("params" to JsonValue.Obj(params.mapValues { JsonValue.Str(it.value) })))
    }

    /** The screens a (valid) package declares, in order (empty = none: the host shows the first template as before). */
    fun list(pkg: JsonValue): List<Resolved> =
        (pkg["screens"]?.array ?: emptyList()).mapNotNull { s -> s["template"]?.string?.let { resolve(pkg, it, null) } }

    /** The screen a host opens for a request (a deep link, a navigate, the default): the requested one, else the first; null without screens. */
    fun resolve(pkg: JsonValue, screen: String?, params: Map<String, String>?): Resolved? {
        val paramsJson = params?.let { p -> JsonValue.Obj(p.mapValues { JsonValue.Str(it.value) }).json }
        val out = JsonValue.parse(runCatching { resolveScreenJson(pkg.json, screen, paramsJson) }.getOrNull() ?: "null")
        val id = out["screen"]?.string ?: return null
        return Resolved(id, out["title"]?.string ?: "", out["icon"]?.string, strings(out["params"]))
    }

    /** An `app.navigate` call's args → the target (screen, params), or the refusal's message. */
    fun target(pkg: JsonValue, args: Map<String, JsonValue>): Result<Pair<String, Map<String, String>>> {
        val out = JsonValue.parse(runCatching { navigateTargetJson(pkg.json, JsonValue.Obj(args).json) }.getOrNull() ?: """{"error":"navigate takes {screen, params?}"}""")
        out["error"]?.string?.let { return Result.failure(IllegalArgumentException(it)) }
        return Result.success((out["screen"]?.string ?: "") to strings(out["params"]))
    }

    private fun strings(v: JsonValue?): Map<String, String> = (v?.obj ?: emptyMap()).mapNotNull { (k, p) -> p.string?.let { k to it } }.toMap()
}

/** What [HostOptions.navigate] receives: the surface, its package, the screen and the params. */
data class NavigateTarget(val surfaceId: String, val packageId: String, val screen: String, val params: Map<String, String>)
