package at.exponential.ui.host

import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.ExponentialUi
import at.exponential.ui.json.JsonValue

/** A user action: a node's `on.<event>` with an `event` action, resolved. */
data class SurfaceActionEvent(
    val surfaceId: String,
    /** The catalog event (`press`, `change`, `select`, `submit`…). */
    val event: String,
    /** The action's `event.name`. */
    val name: String,
    val componentId: String,
    val context: JsonValue,
    /** What the component adds (a Select's `value`, a Tabs' `value`…). */
    val payload: JsonValue?,
)

/**
 * A function call the core does not run itself: an `on.<event>` with
 * `{functionCall: {call, args}}` (A2UI; `function` is the legacy key) to a
 * name outside the catalog's built-ins. An [ExponentialHost] gates it
 * through its policy and runs the registered handler.
 */
data class SurfaceFunctionCallEvent(
    val surfaceId: String,
    val componentId: String,
    /** The function name (`call`). */
    val name: String,
    /** The resolved `args` (an object; empty when the call has none). */
    val args: Map<String, JsonValue>,
)

/** What the media loader fetches: the absolute url and the headers to send (auth for `/api/attachments`). */
data class MediaRequest(val url: String, val headers: Map<String, String> = emptyMap())

/** `change` (debounced while typing) or `commit` (blur / Enter). */
enum class InputKind { Change, Commit }

/**
 * A host-owned input edit. `change` fires debounced (150 ms) while typing
 * with a monotonically increasing `revision`; `commit` on blur / Enter.
 */
data class SurfaceInputEvent(
    val surfaceId: String,
    val componentId: String,
    /** The input's `name` prop. */
    val name: String,
    /** The bound data model pointer, when `value` is a binding. */
    val path: String?,
    val value: JsonValue,
    val revision: Int,
    val kind: InputKind,
)

/** Open `url` through the system (ACTION_VIEW on [ExponentialUi.appContext]); a no-op without a context. */
internal fun systemOpenUrl(url: String) {
    val ctx = ExponentialUi.appContext ?: return
    runCatching {
        ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}

/**
 * What an EMBEDDING APP provides. The SDK knows no transport: actions and
 * input edits are plain values the host forwards wherever it likes. Every
 * member has a default (the [NoHost] behaviour), so a host implements what
 * it needs. Called on the main thread.
 */
interface HostPlugin {
    /**
     * The icon registry: a catalog icon NAME (a registry concept such as
     * `nav-inbox`) → a composable drawn at `size` dp in the current content
     * colour. null = the placeholder circle.
     */
    fun icon(name: String, size: Float): (@Composable () -> Unit)? = null

    /** Every A2UI action. */
    fun onAction(event: SurfaceActionEvent) {}

    /** Host-owned text edits (debounced `change`, `commit` on blur / Enter). */
    fun onInput(event: SurfaceInputEvent) {}

    /**
     * A call to a host function (`functionCall` to a non-built-in name).
     * Default: nothing ([ExponentialHost] routes it through its policy).
     */
    fun onFunctionCall(event: SurfaceFunctionCallEvent) {}

    /** `openUrl` and `Link`. Default: the system opener. */
    fun openUrl(url: String) = systemOpenUrl(url)

    /** Rewrites media URLs (relative attachment paths, signed URLs). */
    fun resolveUrl(src: String): String = src

    /**
     * The request the media loader makes for a picture `src` (images,
     * avatars, video posters): the absolute url plus headers. null = do not
     * load. Default: [resolveUrl], no headers.
     */
    fun mediaRequest(src: String): MediaRequest? = MediaRequest(resolveUrl(src))

    /** Called once per node id for each `Unknown` placeholder. */
    fun onUnknown(component: String, catalogId: String?, id: String) {}

    /**
     * A font family NAME a theme asks for → the Compose family to use (null
     * = the family registered via [ExponentialUi.registerFont], else the
     * default font).
     */
    fun fontFamily(name: String): FontFamily? = null

    /** A richer markdown renderer than the built-in one (null = built-in). */
    fun markdown(text: String, width: Float): (@Composable () -> Unit)? = null
}

/** The host that does nothing (previews, tests). */
object NoHost : HostPlugin

/** A host built from closures (the kitchen sink, quick embeddings). */
class ClosureHost(
    var icons: (String, Float) -> (@Composable () -> Unit)? = { _, _ -> null },
    var actions: (SurfaceActionEvent) -> Unit = {},
    var inputs: (SurfaceInputEvent) -> Unit = {},
    var urls: ((String) -> Unit)? = null,
    var unknowns: (String, String?, String) -> Unit = { _, _, _ -> },
) : HostPlugin {
    override fun icon(name: String, size: Float): (@Composable () -> Unit)? = icons(name, size)
    override fun onAction(event: SurfaceActionEvent) = actions(event)
    override fun onInput(event: SurfaceInputEvent) = inputs(event)
    override fun openUrl(url: String) {
        val u = urls
        if (u != null) u(url) else systemOpenUrl(url)
    }
    override fun onUnknown(component: String, catalogId: String?, id: String) = unknowns(component, catalogId, id)
}
