package at.exponential.ui.model

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.neverEqualPolicy
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.ExponentialUi
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.extension.ExtensionRegistry
import at.exponential.ui.ffi.FfiApplyOutcome
import at.exponential.ui.ffi.FfiLayout
import at.exponential.ui.ffi.Surface
import at.exponential.ui.ffi.coreCatalogId
import at.exponential.ui.ffi.defaultThemeId
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.NoHost
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.str
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.measure.SurfaceMeasurer
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.primitives.PrimitiveTokens
import at.exponential.ui.theme.GeometrySpacing
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.PaintStyle
import at.exponential.ui.theme.PartStyle
import at.exponential.ui.theme.ResolvedTextStyle
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob

/** How overlays present. */
enum class OverlayPresentation {
    /** Dialog / Drawer as sheets, Popover as a popup, DropdownMenu as a menu, Tooltip painted (default). */
    Native,

    /** Everything painted inside the surface at the core's frames with a scrim (snapshots, hosts that own their windows). */
    Painted,
}

/** Options of a surface model. */
data class SurfaceOptions(
    val catalogId: String = coreCatalogId(),
    /** A built-in theme (`exponential` default); null = geometry mode. */
    val theme: ThemeHandle? = ThemeHandle.builtin(defaultThemeId()),
    val mode: Mode = Mode.Dark,
    val overlays: OverlayPresentation = OverlayPresentation.Native,
    /** Round frames to whole dp (off: fractional, like the web). */
    val rounding: Boolean = false,
)

/**
 * ONE surface: owns the core [Surface], runs the layout passes with the
 * painter's measurer, caches the per-structure node data and the per-pass
 * frames and visuals, and holds every bit of interaction state (mirrors,
 * pressed / hover / focus, text fields, open layers). Compose views observe
 * its snapshot state; `ExponentialSurface` is its Compose face. Main thread
 * only. `shaper` builds the [TextShaper] a measured pass uses (never called
 * under [fixedMeasure]); throws the core's `UiException` when the surface
 * cannot be created.
 */
class SurfaceModel(
    val id: String,
    val options: SurfaceOptions = SurfaceOptions(),
    var host: HostPlugin = NoHost,
    scope: CoroutineScope? = null,
    /** The text shaper of measured passes (the views lane hands one built from a fontScale-1 `TextMeasurer`). */
    var shaper: () -> TextShaper,
) {
    /** The core surface. */
    val surface: Surface = Surface.withTheme(id, options.catalogId, options.theme?.theme, options.mode.wire)

    /** The extension registry this surface reads painters from. */
    val extensions: ExtensionRegistry = ExtensionRegistry.shared

    /** Where debounces and delayed closes run. */
    val scope: CoroutineScope = scope ?: CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** Theme family NAMES → Compose families: the host's, else [ExponentialUi.registerFont]'s. */
    val fontResolver: FontResolver = FontResolver { name -> name?.let { host.fontFamily(it) ?: ExponentialUi.fontFamily(it) } }

    private var themeState: ThemeHandle? by mutableStateOf(options.theme)
    private var modeState: Mode by mutableStateOf(options.mode)

    /** The current theme (null = geometry mode). */
    val theme: ThemeHandle? get() = themeState

    /** The current mode. */
    val mode: Mode get() = modeState

    /** Every layout node, pre-order (= paint order = accessibility order). */
    var nodes: List<NodeInfo> by mutableStateOf(emptyList(), neverEqualPolicy())
        private set

    /** Child indices per node. */
    var children: List<List<Int>> by mutableStateOf(emptyList(), neverEqualPolicy())
        private set

    /** Node id → index. */
    var byId: Map<String, Int> = emptyMap()
        private set
    var structureVersion: Long by mutableStateOf(-1L)
        private set

    /** Frames (dp) in SURFACE coordinates by node index (layers included). */
    var frames: List<Rect> by mutableStateOf(emptyList())
        private set
    var styles: List<PaintStyle> by mutableStateOf(emptyList())
        private set

    /** The text colour per node (own, else inherited, else `foreground`). */
    var inks: List<Color> by mutableStateOf(emptyList())
        private set

    /** The resolved text style per LEAF index (read after the pass). */
    var textStyles: Map<Int, ResolvedTextStyle> by mutableStateOf(emptyMap())
        private set
    var layers: List<LayerInfo> by mutableStateOf(emptyList())
        private set
    var lists: Map<String, ListInfo> by mutableStateOf(emptyMap())
        private set
    var surfaceSize: Size by mutableStateOf(Size.Zero)
        private set
    var passCount: Int by mutableIntStateOf(0)
        private set
    var stats: PassStats by mutableStateOf(PassStats())
        private set

    /** Tokens for the generic primitives under the surface's theme. */
    var primitiveTokens: PrimitiveTokens by mutableStateOf(PrimitiveTokens.SYSTEM)
        private set

    /** The host's width (dp, from the view's constraints), visible height and card bound. */
    var width: Float = 0f
        internal set
    var viewportHeight: Float = 0f
        internal set
    var maxHeight: Float? = null
        internal set

    internal var nodesDirty = true
    internal var layoutNeeded = true
    internal val mirrors = HashMap<String, Mirror>()
    internal val interaction = HashMap<String, InteractionState>()
    internal var pressedId: String? = null
    internal val revisions = HashMap<String, Int>()
    internal val fieldsMap = HashMap<String, FieldState>()

    /** The host-owned text fields by node id (`writeGeneration` = reload the view's value). */
    val fields: Map<String, FieldState> get() = fieldsMap

    /** The trigger id that opened each layer owner (focus returns there). */
    val layerReturn = HashMap<String, String>()

    /** The overlay owner dismissed in the last 300 ms (its trigger's own tap must not reopen it). */
    var justDismissed: String? = null
        internal set
    internal var tooltipJob: Job? = null
    internal var measureGeneration = 0L
    internal val unknownReported = HashSet<String>()

    /**
     * Tests and geometry suites: lay out with the core's fixed fake measure
     * (8 px per character, 20 px lines) instead of the text shaper.
     */
    var fixedMeasure = false

    /** The open Select / DatePicker popup (the field node id). */
    var popup: String? by mutableStateOf(null)

    /** The focused text field id. */
    var focusedField: String? by mutableStateOf(null)
        internal set

    /** Slider drags in flight (track id → value). */
    internal val drags = HashMap<String, Double>()

    /** Mirrored controls bumped (views re-read their part visuals). */
    var mirrorGeneration: Int by mutableIntStateOf(0)
        internal set

    init {
        surface.setRounding(options.rounding)
        for (json in extensions.definitions) runCatching { surface.registerExtension(json) }
        primitiveTokens = theme?.primitiveTokens(mode) ?: PrimitiveTokens.SYSTEM
    }

    // Content

    /** One A2UI server→client message (`updateComponents`, `updateDataModel`…). Throws `UiException`. */
    fun apply(message: JsonValue): FfiApplyOutcome = apply(message.json)

    /** One A2UI message as JSON. Throws `UiException`. */
    fun apply(json: String): FfiApplyOutcome {
        val out = surface.apply(json)
        invalidate(out.structureChanged)
        return out
    }

    /** The nested authoring form (fixtures, MCP templates). Throws `UiException`. */
    fun setNested(json: String): FfiApplyOutcome {
        val out = surface.setNested(json)
        invalidate(true)
        return out
    }

    /** A flat component list. Throws `UiException`. */
    fun setComponents(json: String): FfiApplyOutcome {
        val out = surface.setComponents(json)
        invalidate(true)
        return out
    }

    /** Write at a JSON pointer (null removes). */
    fun setData(path: String, value: JsonValue?) {
        runCatching { surface.setData(path, value?.json) }
        invalidate(true)
    }

    /** The data model. */
    val data: JsonValue get() = JsonValue.parse(surface.dataJson())

    /** The surface's validation issues. */
    val issues: List<JsonValue> get() = JsonValue.parse(surface.issuesJson()).array ?: emptyList()

    /** Switch the theme. */
    fun setTheme(theme: ThemeHandle) {
        themeState = theme
        surface.setTheme(theme.theme)
        primitiveTokens = theme.primitiveTokens(mode)
        measureGeneration += 1
        invalidate(true)
    }

    /** Switch light / dark. */
    fun setMode(mode: Mode) {
        modeState = mode
        runCatching { surface.setMode(mode.wire) }
        primitiveTokens = theme?.primitiveTokens(mode) ?: PrimitiveTokens.SYSTEM
        invalidate(true)
    }

    /**
     * Fonts or the shaper's density changed: the core's measure memo is
     * stale (a new measure identity), lay out again.
     */
    fun invalidateMeasures() {
        measureGeneration += 1
        invalidate(false)
    }

    /**
     * Register an extension on THIS surface (the global registry covers new
     * models; see [ExponentialUi.register]). Throws `UiException`.
     */
    fun register(extensionJson: String, painters: Map<String, ExtensionPainter>) {
        surface.registerExtension(extensionJson)
        for ((kind, painter) in painters) extensions.register(kind, painter)
        invalidate(true)
    }

    internal fun invalidate(structure: Boolean) {
        if (structure) nodesDirty = true
        layoutNeeded = true
        if (width > 0f) pass()
    }

    // Viewport

    /**
     * The host's width (dp, from the view's constraints) and visible height
     * (dialog centring, windowed lists). `maxHeight` bounds a card.
     */
    fun setViewport(width: Float, height: Float, maxHeight: Float? = null) {
        val w = kotlin.math.max(0f, kotlin.math.floor(width))
        if (w == this.width && height == viewportHeight && maxHeight == this.maxHeight && !layoutNeeded) return
        this.width = w
        viewportHeight = height
        this.maxHeight = maxHeight
        layoutNeeded = true
        if (w > 0f) pass()
    }

    // The pass

    /** Run one layout pass (the core, the measurer, the caches). */
    fun pass() {
        if (width <= 0f) return
        val t0 = System.nanoTime()
        surface.setViewport(width, viewportHeight, maxHeight)
        if (nodesDirty) readNodes()
        var measurer: SurfaceMeasurer? = null
        val out: FfiLayout = (if (fixedMeasure) runCatching { surface.layoutFixed(null, true) }.getOrNull() else null)
            ?: SurfaceMeasurer(theme, mode, extensions, extensionKinds(), measureGeneration, shaper()).let {
                measurer = it
                surface.layout(it)
            }
        if (out.structureVersion.toLong() != structureVersion || nodesDirty) readNodes()
        val count = nodes.size
        val f = ArrayList<Rect>(count)
        repeat(count) { f.add(Rect.Zero) }
        for (fr in out.frames) {
            val i = fr.index.toInt()
            if (i < count) f[i] = fr.toRect()
        }
        for (l in out.layers) for (fr in l.frames) {
            val i = fr.index.toInt()
            if (i < count) f[i] = fr.toRect()
        }
        frames = f
        val newLayers = out.layers.map(::LayerInfo)
        if (newLayers != layers) layers = newLayers
        val ls = HashMap<String, ListInfo>()
        for (l in out.lists) ls[l.id] = ListInfo(l)
        if (ls != lists) lists = ls
        if (passCount == 0 || out.visualChanges.isNotEmpty() || styles.size != nodes.size) {
            readVisuals()
            // The core resolves a leaf's text style DURING the pass (the
            // recipe's font); read it after, never from the pre-pass nodes.
            readTextStyles()
        }
        val size = Size(out.surfaceWidth, out.surfaceHeight)
        if (size != surfaceSize) surfaceSize = size
        stats = PassStats(out.layoutNs.toLong(), System.nanoTime() - t0, out.upcalls.toInt(), out.measureRounds.toInt(), measurer?.calls ?: 0, nodes.size)
        layoutNeeded = false
        passCount += 1
        layersChanged()
        reportUnknowns()
        echoFields()
    }

    private fun extensionKinds(): Map<String, String> {
        val k = HashMap<String, String>()
        for (n in nodes) n.extensionKind?.let { k[n.id] = it }
        return k
    }

    private fun readNodes() {
        val raw = surface.nodes()
        val list = ArrayList<NodeInfo>(raw.size)
        val ch = ArrayList<MutableList<Int>>(raw.size)
        repeat(raw.size) { ch.add(ArrayList(2)) }
        val ids = HashMap<String, Int>(raw.size * 2)
        for (n in raw) {
            val info = NodeInfo(n)
            info.parent?.let { p -> if (p < ch.size) ch[p].add(info.index) }
            ids[info.id] = info.index
            list.add(info)
        }
        byId = ids
        children = ch
        nodes = list
        structureVersion = surface.structureVersion().toLong()
        nodesDirty = false
        pruneFields()
    }

    private fun readTextStyles() {
        val ts = HashMap<Int, ResolvedTextStyle>()
        for (n in nodes) {
            if (!n.isLeaf) continue
            surface.textStyle(n.index.toUInt())?.let { ts[n.index] = ResolvedTextStyle(it) }
        }
        if (ts != textStyles) textStyles = ts
    }

    private fun readVisuals() {
        val raw = surface.visuals()
        val st = ArrayList<PaintStyle>(kotlin.math.max(raw.size, nodes.size))
        for (v in raw) st.add(PaintStyle(v))
        while (st.size < nodes.size) st.add(PaintStyle.EMPTY)
        styles = st
        val fallback = theme?.ink(mode) ?: if (mode == Mode.Dark) Color.White else Color.Black
        val ink = ArrayList<Color>(nodes.size)
        repeat(nodes.size) { ink.add(fallback) }
        for (n in nodes) {
            val c = st[n.index].color
            val p = n.parent
            if (c != null) ink[n.index] = c else if (p != null && p < n.index) ink[n.index] = ink[p]
        }
        inks = ink
    }

    private fun reportUnknowns() {
        for (n in nodes) {
            if (n.component != "Unknown" || unknownReported.contains(n.id)) continue
            unknownReported.add(n.id)
            host.onUnknown(n.props.str("component").ifEmpty { n.component }, n.catalogId ?: n.props.str("catalogId"), n.id)
        }
    }

    // Lookups

    /** The index of the node `id`. */
    fun indexOf(id: String): Int? = byId[id]

    /** The node at `index`. */
    fun node(index: Int): NodeInfo? = nodes.getOrNull(index)

    /** The frame (dp, surface coordinates) of `index`. */
    fun frame(index: Int): Rect = frames.getOrNull(index) ?: Rect.Zero

    /** The core's resolved box style of `index`. */
    fun style(index: Int): PaintStyle = styles.getOrNull(index) ?: PaintStyle.EMPTY

    /** The text colour of `index`. */
    fun ink(index: Int): Color = inks.getOrNull(index) ?: (theme?.ink(mode) ?: if (mode == Mode.Dark) Color.White else Color.Black)

    /** The text style of a leaf (valid after a pass). */
    fun textStyle(index: Int): ResolvedTextStyle = textStyles[index] ?: ResolvedTextStyle.BODY

    /** The owner node of a synthetic part (else the node itself). */
    fun owner(index: Int): NodeInfo? {
        val n = node(index) ?: return null
        val o = n.owner ?: return n
        return byId[o]?.let { nodes[it] } ?: n
    }

    /** The owner's props. */
    fun ownerProps(index: Int): Props = owner(index)?.props ?: emptyMap()

    /** A part's look under the surface's theme (empty in geometry mode). */
    fun part(component: String, part: String, props: Props, states: List<String> = emptyList()): PartStyle =
        theme?.part(component, part, props, states, mode) ?: PartStyle.EMPTY

    /** A colour token. */
    fun color(name: String): Color? = theme?.color(name, mode)

    /** A spacing token (geometry fallbacks without a theme). */
    fun spacing(name: String): Float = theme?.spacing(name) ?: GeometrySpacing.value(name)

    /** A control size token. */
    fun control(name: String, fallback: Float): Float = theme?.control(name, fallback) ?: fallback

    /** The interaction states reported for `id`. */
    fun statesOf(id: String): List<String> = interaction[id]?.states ?: emptyList()

    /** Is a node (or its owner, label parts aside) disabled? */
    fun isDisabled(index: Int): Boolean {
        val n = node(index) ?: return true
        if (n.disabled) return true
        if (n.part != "label") {
            val o = owner(index)
            if (o != null && o.index != n.index && o.disabled) return true
        }
        return false
    }

    /** The content height a scrolling container shows (its children's extent). */
    fun contentHeight(index: Int): Float {
        val n = node(index)
        if (n != null) lists[n.id]?.let { if (it.windowed) return it.contentHeight }
        val top = frame(index).top
        return children.getOrNull(index)?.maxOfOrNull { frame(it).bottom - top } ?: 0f
    }

    /**
     * The accessible name of a pressable container: its visible leaves'
     * labels in pre-order, comma-joined (what TalkBack reads for a row).
     */
    fun combinedLabel(index: Int): String {
        val parts = ArrayList<String>()
        fun walk(i: Int) {
            val n = node(i) ?: return
            if (n.hidden) return
            if (n.isLeaf) {
                val l = n.accessibilityLabel
                if (!l.isNullOrEmpty() && (n.component != "Icon" || n.props["label"] != null)) parts.add(l)
            } else {
                for (c in children.getOrNull(i) ?: emptyList()) walk(c)
            }
        }
        walk(index)
        return parts.joinToString(", ")
    }

    /**
     * The box style a node paints with; mirrored controls (unbound
     * checkboxes, switches, radios, toggles, open triggers) re-resolve theirs.
     * Reads [mirrorGeneration] and [popup], so a view recomposes on a mirror.
     */
    fun boxStyle(index: Int): PaintStyle {
        @Suppress("UNUSED_VARIABLE") val gen = mirrorGeneration
        val base = style(index)
        val n = node(index) ?: return base
        if (theme == null) return base
        val states = statesOf(n.id)
        val rc = n.recipeComponent
        return when {
            (rc == "Checkbox" && n.part == "box") || (rc == "Switch" && n.part == "track") -> {
                val checked = checked(index)
                val external = (owner(index)?.props?.get("checked") ?: JsonValue.Bool(false)).bool ?: false
                if (checked == external) return base
                val props = ownerProps(index) + ("checked" to JsonValue.Bool(checked))
                val st = if (checked) states + "checked" else states
                part(rc, n.part, props, st).style
            }
            rc == "Radio" && n.part == "dot" -> {
                val st = if (radioChecked(index)) states + "checked" else states
                part("Radio", "item", ownerProps(index), st).style
            }
            rc == "Toggle" && n.part == null -> {
                val pressed = togglePressed(index)
                if (pressed == (n.props["pressed"]?.bool ?: false)) return base
                part("Toggle", "root", n.props + ("pressed" to JsonValue.Bool(pressed)), states).style
            }
            (rc == "Select" || rc == "DatePicker") && n.part == "field" -> {
                val st = if (popup == n.id) states + "open" else states
                part(rc, "trigger", ownerProps(index), st).style
            }
            else -> base
        }
    }

    /**
     * Does the DropdownMenu `owner` paint its items (recipe `native: false`)
     * instead of opening a native menu?
     */
    fun menuPainted(owner: String): Boolean {
        val n = indexOf(owner)?.let(::node) ?: return false
        return part("DropdownMenu", "content", n.props).native == false
    }

    /** Does a Select paint its popup (recipe `native: false`)? The native menu is the default. */
    fun selectPainted(ownerProps: Props): Boolean = part("Select", "content", ownerProps).native == false

    /** Is `layer` painted inside the surface (Tooltip; every layer under [OverlayPresentation.Painted])? */
    fun paintsInSurface(layer: LayerInfo): Boolean {
        if (options.overlays == OverlayPresentation.Painted) return true
        return when (layer.kind) {
            "Tooltip" -> true
            else -> false
        }
    }

    /** The family of a theme family NAME through the host and the registry (null = default). */
    fun fontFamily(name: String?): FontFamily? = fontResolver.family(name)

    companion object {
        private fun roundHalfAway(x: Double): Double = if (x >= 0) kotlin.math.floor(x + 0.5) else -kotlin.math.floor(-x + 0.5)

        /** `v` snapped to `min + k·step` and clamped. */
        fun snap(v: Double, min: Double, max: Double, step: Double): Double {
            var x = if (step > 0) min + roundHalfAway((v - min) / step) * step else v
            x = kotlin.math.min(kotlin.math.max(x, kotlin.math.min(min, max)), kotlin.math.max(min, max))
            return roundHalfAway(x * 1e9) / 1e9
        }
    }
}
