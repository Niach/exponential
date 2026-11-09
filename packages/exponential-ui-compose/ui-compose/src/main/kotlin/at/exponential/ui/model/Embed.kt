package at.exponential.ui.model

import androidx.compose.runtime.Composable
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.ffi.UiException
import at.exponential.ui.host.A2UI_VERSION
import at.exponential.ui.host.AppFileLoader
import at.exponential.ui.host.FilePickRequest
import at.exponential.ui.host.HostAttachment
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.ImageSize
import at.exponential.ui.host.MediaOptions
import at.exponential.ui.host.PaintError
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.host.SurfaceFunctionCallEvent
import at.exponential.ui.host.SurfaceInputEvent
import at.exponential.ui.host.TextLinkSegment
import at.exponential.ui.host.UrlPolicy
import at.exponential.ui.json.JsonValue

// #EDE-65 lane M (fixtures/embed.json, docs/round-9-contract.md §19): Embed,
// a nested genUI surface drawn in place, SANDBOXED. The pure half is the TS
// reference's twin (`src/embed.ts`: embedCard / embedMount / embedPayload /
// embedSurfaceId); the mount half gives each painted Embed its OWN
// [SurfaceModel] (own data model, own ids, surface id `<host>/<embed id>`)
// under the host's theme, mode, settings, icons, media and url policy,
// whose `event` actions fire the host Embed's `action` and whose host
// functions are denied.

/** A nested card: the root and its OWN data model (never the host's). */
data class EmbedCard(val root: JsonValue, val data: Map<String, JsonValue>)

/** What a bad `surface` paints instead (`fixtures/embed.json` `errorRoot`): an error Alert; the host around it keeps painting. */
val EMBED_ERROR_ROOT: JsonValue = JsonValue.Obj(
    mapOf(
        "id" to JsonValue.Str("embed_error"),
        "component" to JsonValue.Str("Alert"),
        "props" to JsonValue.Obj(mapOf("type" to JsonValue.Str("error"), "title" to JsonValue.Str("\$string.embedInvalid"))),
    ),
)

/** The Embed's one event: every inner `event` action reaches the host as it. */
const val EMBED_EVENT: String = "action"

/** A structurally sound nested node: string `id` + `component`, `props` / `on` / `style` objects, `children` an array of nodes, `slots` an object of nodes, recursively. */
private fun soundNode(n: JsonValue?, depth: Int): Boolean {
    val o = (n as? JsonValue.Obj)?.v ?: return false
    if (depth > 64) return false
    val id = o["id"]?.string
    val component = o["component"]?.string
    if (id.isNullOrEmpty() || component.isNullOrEmpty()) return false
    for (key in listOf("props", "on", "style")) {
        val v = o[key]
        if (v != null && v !is JsonValue.Obj) return false
    }
    val children = o["children"]
    if (children != null && (children !is JsonValue.Arr || !children.v.all { soundNode(it, depth + 1) })) return false
    val slots = o["slots"]
    if (slots != null && (slots !is JsonValue.Obj || !slots.v.values.all { soundNode(it, depth + 1) })) return false
    return true
}

/**
 * The Embed's resolved `surface` prop → its card, or null when it is not
 * one (not an object, no sound `root`, a non-object `data`, a `version`
 * other than 1.0): the painter then shows [EMBED_ERROR_ROOT]. A missing or
 * null `data` is an empty model. An unknown component inside a sound root
 * is NOT bad: the nested surface paints its Unknown placeholder. A missing
 * prop (an unbound `{"@path"}`) is Kotlin `null`, a JSON null [JsonValue.Null]:
 * neither is a card.
 */
fun embedCard(surface: JsonValue?): EmbedCard? {
    val o = (surface as? JsonValue.Obj)?.v ?: return null
    val version = o["version"]
    if (version != null && version != JsonValue.Str(A2UI_VERSION)) return null
    if (!soundNode(o["root"], 1)) return null
    val data = o["data"]
    if (data != null && data !is JsonValue.Null && data !is JsonValue.Obj) return null
    return EmbedCard(o.getValue("root"), (data as? JsonValue.Obj)?.v ?: emptyMap())
}

/** What a painter mounts for `surface`: the card ([valid]), else the error box. */
data class EmbedMount(val card: EmbedCard, val valid: Boolean)

/** The card a painter mounts for `surface`: the card, else the error box. */
fun embedMount(surface: JsonValue?): EmbedMount {
    val card = embedCard(surface)
    return if (card != null) EmbedMount(card, true) else EmbedMount(EmbedCard(EMBED_ERROR_ROOT, emptyMap()), false)
}

/**
 * An inner event → the Embed's `action` payload (merged over the author's
 * context like any component payload, by the core): the inner name and
 * context.
 */
fun embedPayload(name: String, context: JsonValue?): JsonValue =
    JsonValue.Obj(linkedMapOf("name" to JsonValue.Str(name), "context" to JsonValue.Obj(context?.obj ?: emptyMap())))

/** The nested surface's id: the host's surface id and the Embed's (painted) component id, so two cards' inner ids never meet. */
fun embedSurfaceId(hostSurfaceId: String, componentId: String): String = "$hostSurfaceId/$componentId"

// The mount

/**
 * One painted Embed's nested surface: the model, the `surface` value it
 * was mounted from (another value = a fresh mount) and the heights the host
 * measured it at, per width (a nested pass that moves the height at a
 * width the host measured asks the host to measure the Embed again).
 */
class EmbedSlot internal constructor(
    /** The Embed's painted component id (a template row's instance id). */
    val componentId: String,
    /** The resolved `surface` value this mount shows. */
    val surface: JsonValue?,
) {
    /** The nested surface. */
    lateinit var model: SurfaceModel
        internal set

    /** Whether [surface] is a card (else the error box shows). */
    var valid: Boolean = false
        internal set
    internal var measuring = false

    /** Width (whole dp, as the nested surface lays out) → the height the host measured. */
    internal val measured = HashMap<Float, Float>()
}

/**
 * The nested surface's host: the outer surface's host for everything a
 * card shares with it (icons, fonts, the url and media policy, markdown,
 * the clipboard, paint errors), the sandbox for the rest. Every inner
 * `event` action fires the outer Embed's `action` ([embedPayload]); an
 * inner host function is DENIED (never forwarded, and its event does not
 * fire); inner input edits stay inner (the card's own model).
 */
internal class EmbedSandbox(private val outer: SurfaceModel, private val slot: EmbedSlot) : HostPlugin {
    private val base: HostPlugin get() = outer.host

    override fun icon(name: String, size: Float): (@Composable () -> Unit)? = base.icon(name, size)

    override fun afterLayout(surfaceId: String) {
        if (slot.measuring) return
        val nested = slot.model
        val known = slot.measured[nested.width] ?: return
        val h = nested.surfaceSize.height
        if (kotlin.math.abs(h - known) < 0.5f) return
        // The card grew or shrank on its own (an inner `set`): the host
        // measures the Embed again (and re-fills what it measured).
        slot.measured.clear()
        outer.indexOf(slot.componentId)?.let(outer::remeasure)
    }

    override fun onAction(event: SurfaceActionEvent) {
        val index = outer.indexOf(slot.componentId) ?: return
        if (outer.node(index)?.component != "Embed") return
        outer.fire(index, EMBED_EVENT, embedPayload(event.name, event.context))
    }

    override fun onInput(event: SurfaceInputEvent) {}

    override fun onFunctionCall(event: SurfaceFunctionCallEvent) {}

    override fun onFunctionCallThen(event: SurfaceFunctionCallEvent, then: () -> Unit) {}

    override val urlPolicy: UrlPolicy? get() = base.urlPolicy

    override val mediaOptions: MediaOptions? get() = base.mediaOptions

    override fun openUrl(url: String) = base.openUrl(url)

    override fun textLinks(text: String): List<TextLinkSegment>? = base.textLinks(text)

    override fun imageSize(src: String): ImageSize? = base.imageSize(src)

    override val onImagePress: ((String) -> Unit)? get() = base.onImagePress

    override fun attachment(href: String): HostAttachment? = base.attachment(href)

    override val markdownOptions: at.exponential.ui.primitives.MarkdownOptions get() = base.markdownOptions

    override fun resolveUrl(src: String): String = base.resolveUrl(src)

    override val appFile: AppFileLoader? get() = base.appFile

    override fun onPaintError(error: PaintError) = base.onPaintError(error)

    override fun onUnknown(component: String, catalogId: String?, id: String) = base.onUnknown(component, catalogId, id)

    override fun fontFamily(name: String): FontFamily? = base.fontFamily(name)

    override fun markdown(text: String, width: Float): (@Composable () -> Unit)? = base.markdown(text, width)

    override fun copy(text: String) = base.copy(text)

    override fun pickFiles(request: FilePickRequest) = base.pickFiles(request)
}

/**
 * The nested surface of Embed [componentId] showing [surface]: the mount
 * it has while the value is the same, else a FRESH one (its own model,
 * nothing of the old card kept). Follows the host's theme, mode and
 * settings on every ask.
 */
internal fun SurfaceModel.embedSlot(componentId: String, surface: JsonValue?): EmbedSlot {
    val kept = embeds[componentId]
    if (kept != null && kept.surface == surface) {
        syncEmbed(kept.model)
        return kept
    }
    val mount = embedMount(surface)
    val slot = EmbedSlot(componentId, surface)
    val outer = this
    val nestedOptions = SurfaceOptions(
        catalogId = this.options.catalogId,
        theme = theme,
        mode = mode,
        overlays = this.options.overlays,
        rounding = this.options.rounding,
        settings = settings,
    )
    val nested = SurfaceModel(embedSurfaceId(id, componentId), nestedOptions, EmbedSandbox(outer, slot), scope) { outer.shaper() }
    slot.model = nested
    nested.fixedMeasure = fixedMeasure
    nested.setEnvironment(environment)
    slot.valid = mount.valid && try {
        if (mount.card.data.isNotEmpty()) nested.setData("", JsonValue.Obj(mount.card.data))
        nested.setNested(mount.card.root.json)
        true
    } catch (_: UiException) {
        false
    }
    // A card the core refuses is a bad surface too: the error box.
    if (!slot.valid) nested.setNested(EMBED_ERROR_ROOT.json)
    embeds[componentId] = slot
    return slot
}

/**
 * The host's theme, mode and settings onto a nested surface (a no-op when
 * they are the same). The environment (the device's) is copied at the mount;
 * a painted card's own `ExponentialSurface` reads the same device after.
 */
private fun SurfaceModel.syncEmbed(nested: SurfaceModel) {
    val t = theme
    if (t != null && nested.theme !== t) nested.setTheme(t)
    if (nested.mode != mode) nested.setMode(mode)
    nested.setSettings(settings)
}

/**
 * The nested surface of the Embed at layout node [index], mounted on the
 * first ask (from the node's resolved `surface`) and laid out at the
 * Embed's frame width (else the host's): what the painter composes and
 * what tests press. null = not an Embed.
 */
fun SurfaceModel.embed(index: Int): SurfaceModel? {
    val n = node(index)?.takeIf { it.component == "Embed" && !it.removed } ?: return null
    val slot = embedSlot(n.id, n.props["surface"])
    val nested = slot.model
    if (nested.width <= 0f) {
        val w = frame(index).width.takeIf { it > 0f } ?: width
        if (w > 0f) nested.setSurfaceWidth(w)
    }
    return nested
}

/** The mounted nested surface of Embed [componentId] (null = none mounted). */
fun SurfaceModel.embedModel(componentId: String): SurfaceModel? = embeds[componentId]?.model

/**
 * The measurer's question about Embed [componentId] (its resolved
 * [surface]) at content width [wrap] (null = max-content, 0 = min-content):
 * the nested surface laid out at that width (max-content = the host's
 * width: a card fills what it gets), its height the card's.
 */
internal fun SurfaceModel.embedSize(componentId: String, surface: JsonValue?, wrap: Float?): Size {
    val slot = embedSlot(componentId, surface)
    val w = wrap?.takeIf { it > 0f } ?: width
    if (w <= 0f) return Size.Zero
    val nested = slot.model
    slot.measuring = true
    try {
        nested.setSurfaceWidth(w)
    } finally {
        slot.measuring = false
    }
    val h = nested.surfaceSize.height
    slot.measured[nested.width] = h
    return Size(if (wrap != null && wrap <= 0f) 0f else w, h)
}

/** Forget the nested surfaces of Embeds that left the host (after a pass). */
internal fun SurfaceModel.pruneEmbeds() {
    if (embeds.isEmpty()) return
    embeds.keys.retainAll { id -> byId[id]?.let { node(it) }?.let { it.component == "Embed" && !it.removed } == true }
}
