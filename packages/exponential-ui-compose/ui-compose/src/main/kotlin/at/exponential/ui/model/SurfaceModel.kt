package at.exponential.ui.model

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.neverEqualPolicy
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.ExponentialUi
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.extension.ExtensionRegistry
import at.exponential.ui.format.IcuFormatter
import at.exponential.ui.ffi.FfiApplyOutcome
import at.exponential.ui.ffi.FfiLayout
import at.exponential.ui.ffi.FfiSettings
import at.exponential.ui.ffi.HostFormatter
import at.exponential.ui.ffi.Surface
import at.exponential.ui.ffi.coreCatalogId
import at.exponential.ui.ffi.defaultThemeId
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.MediaRequest
import at.exponential.ui.host.PaintError
import at.exponential.ui.host.policedMediaRequest
import at.exponential.ui.host.safeHref
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
import kotlin.math.abs
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.SupervisorJob

/** How overlays present. */
enum class OverlayPresentation {
    /** Dialog / Drawer as sheets, Popover and Menu (the core's layers) as popups, Tooltip painted (default). */
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
    /** Locale, strings, density, contrast, motion, insets (round 1 §4, round 2 §3). */
    val settings: SurfaceSettings = SurfaceSettings(),
)

/**
 * The surface settings a host chooses. `null` members follow the device
 * ([SurfaceEnvironment], read by `ExponentialSurface`): its locale, time
 * zone, font scale, animator scale (0 = reduced motion), pointer and
 * safe-area insets.
 */
data class SurfaceSettings(
    /** BCP 47; null = the device's (`en-US` headless). */
    val locale: String? = null,
    /** IANA; null = the device's (`UTC` headless). */
    val timeZone: String? = null,
    /** Built-in string overrides `{id: text}` (`catalog/strings.json`). */
    val strings: Map<String, String> = emptyMap(),
    /** The mode follows the platform's dark setting (`system`); else [SurfaceOptions.mode]. */
    val followSystemMode: Boolean = false,
    /** `compact | default | comfortable` (scales control and spacing tokens). */
    val density: String = "default",
    /** `normal | high | system` (the theme's contrast overlays). */
    val contrast: String = "normal",
    /** Text scale (1 = the theme's sizes); null = the device's font scale. */
    val fontScale: Float? = null,
    /** null = the device's (animator duration scale 0). */
    val reducedMotion: Boolean? = null,
    /** A hover-capable pointer; null = the device's (a mouse is connected). */
    val hover: Boolean? = null,
    /** Safe-area insets (dp) layers keep clear of; null = the window's. */
    val insets: Insets? = null,
    /** The calendar's today (`yyyy-mm-dd`); null = the clock's. */
    val today: String? = null,
    /** How long a hover card / tooltip stays after it is left (ms). */
    val hoverCloseMs: Int = 0,
    /** The formatter; null = [IcuFormatter] in the locale and time zone. */
    val formatter: HostFormatter? = null,
)

/** Safe-area insets in dp. */
data class Insets(val top: Float = 0f, val right: Float = 0f, val bottom: Float = 0f, val left: Float = 0f)

/** What the device says (`ExponentialSurface` reads it; tests and headless hosts keep the defaults). */
data class SurfaceEnvironment(
    val locale: String = "en-US",
    val timeZone: String = "UTC",
    val systemDark: Boolean = false,
    val systemHighContrast: Boolean = false,
    val fontScale: Float = 1f,
    val reducedMotion: Boolean = false,
    val hover: Boolean = false,
    val insets: Insets = Insets(),
)

/** A toast the core keeps open: its id (the Toast node) and how long it shows (0 = sticky). */
data class ToastInfo(val id: String, val durationMs: Double, val kind: String)

/** A scroll container's state (the core's `FfiScroll`): unscrolled frames, the offset, the content size (dp). */
data class ScrollInfo(
    val index: Int,
    val offsetX: Float,
    val offsetY: Float,
    val contentWidth: Float,
    val contentHeight: Float,
    val scrollX: Boolean,
    val scrollY: Boolean,
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

    /**
     * The theme the core resolves against: [theme] with `extends`, the
     * density and the contrast applied (`Surface.effectiveTheme`). Every
     * token and sub-part lookup reads it, so a painted part matches the
     * core's layout under `compact` or high contrast.
     */
    var effectiveTheme: ThemeHandle? by mutableStateOf(options.theme)
        private set
    private var effectiveKey: String? = null

    /** Re-read [effectiveTheme] when the theme, density or contrast moved. */
    private fun refreshEffectiveTheme() {
        val t = themeState
        val st = appliedSettings
        val key = "${System.identityHashCode(t)}|${st?.density}|${st?.contrast}|${st?.systemHighContrast}"
        if (key == effectiveKey) return
        effectiveKey = key
        effectiveTheme = if (t == null) null else runCatching { surface.effectiveTheme() }.getOrNull()?.let(::ThemeHandle) ?: t
    }

    /** The current mode (`system` resolved). */
    val mode: Mode get() = modeState

    /** The host's settings (see [setSettings]). */
    var settings: SurfaceSettings = options.settings
        private set

    /** What the device says (see [setEnvironment]). */
    var environment: SurfaceEnvironment = SurfaceEnvironment()
        private set

    /** The surface's formatter (the host's, else ICU in the locale and zone; null = the core's English fallback). */
    var formatter: HostFormatter? = null
        private set
    private var formatterKey: String? = null
    private var appliedSettings: FfiSettings? = null

    /** The built-in string table in effect (`{id: text}`, the host's overrides applied). */
    var strings: Map<String, String> = emptyMap()
        private set

    /** Scroll containers by node index (unscrolled frames; translate descendants by the offset). */
    var scrolls: Map<Int, ScrollInfo> by mutableStateOf(emptyMap())
        private set

    /**
     * Bumped when a pass finds the core's offset of a scroll container away
     * from what its view shows (`scrollIntoView`, `scrollToIndex`, a clamp):
     * the views re-read [scrolls] and follow.
     */
    var scrollEpoch: Int by mutableIntStateOf(0)
        private set

    /** What each scroll container's VIEW shows (dp), as it reported it ([scrollTo] `fromView`). */
    internal val viewScroll = HashMap<Int, Offset>()

    /** Nodes whose paint follows a scroll offset: `position: sticky` boxes, sticky List / Table headers. */
    internal var scrollPinned: List<Int> = emptyList()
        private set
    private var pinnedDirty = true

    /** Pinned (`position: sticky`, sticky List headers) nodes: index → the offset (dp) on top of their frame. */
    var sticky: Map<Int, Offset> by mutableStateOf(emptyMap())
        private set

    /** The toasts the core keeps open (timers run in [toastTimers]). */
    var toasts: List<ToastInfo> by mutableStateOf(emptyList())
        private set

    /** The surface direction the last pass laid out with (`ltr | rtl`). */
    var direction: String by mutableStateOf("ltr")
        private set

    /** The breakpoint the last pass resolved (`sm`, `md`…; null below the first). */
    var breakpoint: String? by mutableStateOf(null)
        private set

    /** A focus the core asked for (a Form's first invalid field, the `focus` command): the node id, consumed by its view. */
    var focusRequest: String? by mutableStateOf(null)

    /** The keyboard-focused node (hardware keys; `focus-visible`). */
    var keyboardFocus: String? by mutableStateOf(null)
        internal set

    /** Speaks through the platform (set by `ExponentialSurface`: the view's announcement). */
    var announcer: ((text: String, live: String) -> Unit)? = null

    /** The host viewport's scroll of the whole surface (dp), reported by `ExponentialSurface`. */
    var surfaceScroll: Offset by mutableStateOf(Offset.Zero)
        internal set

    /** Asks the HOST to scroll the surface to (x, y) (a `scrollToIndex` on a list the page scrolls), set by `ExponentialSurface`. */
    var scrollSurfaceHandler: ((x: Float, y: Float) -> Unit)? = null

    internal val toastTimers = HashMap<String, Job>()

    /** Toast id → what holds it (`hover`, `press`, `focus`): its timer pauses while any does. */
    internal val toastHeld = HashMap<String, MutableSet<String>>()

    /** Toast id → the ms its timer still has to run (a hold keeps the rest, never restarts the full duration). */
    internal val toastLeft = HashMap<String, Long>()
    internal val hoverTimers = HashMap<String, Job>()

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

    /**
     * Conformance / tests: when set, every node view records its index here
     * as it composes (in composition order = paint order).
     */
    var paintTrace: MutableCollection<Int>? = null

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
        // A registered extension the core refuses fails the surface (the
        // host learns why), never a surface silently missing its components.
        for (json in extensions.definitions) surface.registerExtension(json)
        applySettings()
        refreshEffectiveTheme()
        primitiveTokens = effectiveTheme?.primitiveTokens(mode) ?: PrimitiveTokens.SYSTEM
    }

    // Settings

    /** Replace the host's settings (takes effect on the next pass). */
    fun setSettings(settings: SurfaceSettings) {
        if (settings == this.settings) return
        this.settings = settings
        if (applySettings()) invalidate(true)
    }

    /** What the device says changed (locale, zone, dark, font scale, motion, pointer, insets). */
    fun setEnvironment(environment: SurfaceEnvironment) {
        if (environment == this.environment) return
        this.environment = environment
        if (applySettings()) invalidate(true)
    }

    /**
     * Settings ⊕ environment → the core's `SurfaceSettings` and the
     * formatter. Returns whether anything reached the core.
     */
    private fun applySettings(): Boolean {
        val st = settings
        val env = environment
        val locale = st.locale ?: env.locale
        val zone = st.timeZone ?: env.timeZone
        val wanted = FfiSettings(
            locale = locale,
            stringsJson = JsonValue.Obj(st.strings.mapValues { JsonValue.Str(it.value) }).json,
            mode = if (st.followSystemMode) "system" else modeState.wire,
            systemDark = env.systemDark,
            density = st.density,
            contrast = st.contrast,
            systemHighContrast = env.systemHighContrast,
            fontScale = st.fontScale ?: env.fontScale,
            hover = st.hover ?: env.hover,
            reducedMotion = st.reducedMotion ?: env.reducedMotion,
            insetTop = (st.insets ?: env.insets).top,
            insetRight = (st.insets ?: env.insets).right,
            insetBottom = (st.insets ?: env.insets).bottom,
            insetLeft = (st.insets ?: env.insets).left,
            today = st.today,
            hoverCloseMs = st.hoverCloseMs.coerceAtLeast(0).toUInt(),
            timeZone = zone,
        )
        val key = "$locale|$zone|${System.identityHashCode(st.formatter)}"
        var changed = false
        if (key != formatterKey) {
            formatterKey = key
            formatter = st.formatter ?: IcuFormatter.create(locale, zone)
            surface.setFormatter(formatter)
            changed = true
        }
        if (wanted != appliedSettings) {
            runCatching { surface.setSettings(wanted) }.onSuccess {
                appliedSettings = wanted
                changed = true
            }
            strings = runCatching { JsonValue.parse(surface.stringsJson()).obj?.mapValues { it.value.string ?: "" } }.getOrNull() ?: emptyMap()
            val before = effectiveTheme
            refreshEffectiveTheme()
            if (effectiveTheme !== before) primitiveTokens = effectiveTheme?.primitiveTokens(modeState) ?: PrimitiveTokens.SYSTEM
            val resolved = Mode.of(surface.mode()) ?: modeState
            if (resolved != modeState) {
                modeState = resolved
                primitiveTokens = effectiveTheme?.primitiveTokens(resolved) ?: PrimitiveTokens.SYSTEM
            }
        }
        return changed
    }

    /** A built-in string (`$string.<id>`, the host's overrides applied), `{name}` placeholders filled. */
    fun string(id: String, params: Map<String, Any?> = emptyMap()): String {
        var s = strings[id] ?: id
        for ((k, v) in params) s = s.replace("{$k}", v?.toString() ?: "")
        return s
    }

    /** Reduced motion in effect (the host's setting, else the device's). */
    val reducedMotion: Boolean get() = settings.reducedMotion ?: environment.reducedMotion

    // Content

    /** One A2UI server→client message (`updateComponents`, `updateDataModel`…). Throws `UiException`. */
    fun apply(message: JsonValue): FfiApplyOutcome = apply(message.json)

    /** One A2UI message as JSON. Throws `UiException`. */
    fun apply(json: String): FfiApplyOutcome {
        val out = surface.apply(json)
        if (out.structureChanged) forgetPaintFailures()
        invalidate(out.structureChanged)
        return out
    }

    /** The nested authoring form (fixtures, MCP templates). Throws `UiException`. */
    fun setNested(json: String): FfiApplyOutcome {
        val out = surface.setNested(json)
        forgetPaintFailures()
        invalidate(true)
        return out
    }

    /** A flat component list. Throws `UiException`. */
    fun setComponents(json: String): FfiApplyOutcome {
        val out = surface.setComponents(json)
        forgetPaintFailures()
        invalidate(true)
        return out
    }

    // Hrefs, media, painter failures (catalog/host.json urls / media / paint)

    /** The href the host's URL policy allows for `url` (resolved), else null: the link paints as plain text. */
    fun href(url: String): String? = safeHref(host, url)

    /** Open `url` when the URL policy allows it (every Link, markdown link and `openUrl`). */
    fun openHref(url: String) {
        href(url)?.let(host::openUrl)
    }

    /** The media request for `src` (the host's `resolveUrl`, then the media policy); null = nothing loads. */
    fun mediaRequest(src: String): MediaRequest? = policedMediaRequest(host, src)

    /** Components whose painter failed → the message: they paint an empty box. */
    val paintFailures = mutableStateMapOf<String, String>()
    private val reportedPaintErrors = HashSet<Pair<String, String>>()

    /**
     * Component `componentId`'s painter failed: it paints an empty box from
     * now on and the host hears `onPaintError` once per component + message.
     */
    fun paintFailed(componentId: String, message: String) {
        if (paintFailures[componentId] != message) paintFailures[componentId] = message
        if (reportedPaintErrors.add(componentId to message)) host.onPaintError(PaintError(id, componentId, message))
    }

    private fun forgetPaintFailures() {
        reportedPaintErrors.clear()
        if (paintFailures.isNotEmpty()) paintFailures.clear()
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
        refreshEffectiveTheme()
        primitiveTokens = effectiveTheme?.primitiveTokens(mode) ?: PrimitiveTokens.SYSTEM
        measureGeneration += 1
        invalidate(true)
    }

    /** Switch light / dark (a `followSystemMode` setting keeps following the platform). */
    fun setMode(mode: Mode) {
        modeState = mode
        if (settings.followSystemMode) {
            applySettings()
        } else {
            runCatching { surface.setMode(mode.wire) }
            appliedSettings = appliedSettings?.copy(mode = mode.wire)
        }
        primitiveTokens = effectiveTheme?.primitiveTokens(this.mode) ?: PrimitiveTokens.SYSTEM
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
        if (height > 0f) hostViewport = true
        applyViewport(width, height, maxHeight)
    }

    /** The host set the visible height itself ([setViewport]): the surface's own measurement no longer moves it. */
    private var hostViewport = false

    /** The surface's width from its own layout (`ExponentialSurface`). */
    internal fun setSurfaceWidth(width: Float) = applyViewport(width, viewportHeight, maxHeight)

    /**
     * The visible part of the surface in the window (dp), unless the host
     * set the viewport. It moves on every frame of a host scroll while an
     * edge of the surface is on screen, and each change is a full pass: the
     * first value and growth by ≥ [VIEWPORT_STEP] apply at once (a windowed
     * list never shows blank rows), anything else once the scroll rests
     * ([VIEWPORT_SETTLE_MS]); a larger height than needed only renders a
     * few more rows.
     */
    internal fun autoViewportHeight(height: Float) {
        if (hostViewport) return
        viewportJob?.cancel()
        viewportJob = null
        val d = height - viewportHeight
        if (abs(d) < 0.5f) return
        if (viewportHeight <= 0f || d >= VIEWPORT_STEP) {
            applyViewport(width, height, maxHeight)
            return
        }
        viewportJob = scope.launch {
            delay(VIEWPORT_SETTLE_MS)
            viewportJob = null
            if (!hostViewport) applyViewport(width, height, maxHeight)
        }
    }

    private var viewportJob: Job? = null

    private fun applyViewport(width: Float, height: Float, maxHeight: Float?) {
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
            ?: SurfaceMeasurer(effectiveTheme, mode, extensions, extensionKinds(), measureGeneration, textShaper(), liveTexts(), ::ownerComponentOf, ::paintFailed) { mediaRequest(it) != null }.let {
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
        val sc = HashMap<Int, ScrollInfo>()
        for (x in out.scrolls) sc[x.index.toInt()] = ScrollInfo(x.index.toInt(), x.offsetX, x.offsetY, x.contentWidth, x.contentHeight, x.scrollX, x.scrollY)
        if (sc != scrolls) scrolls = sc
        viewScroll.keys.retainAll(sc.keys)
        if (sc.any { (i, x) -> viewScroll[i]?.let { v -> abs(v.x - x.offsetX) > 0.5f || abs(v.y - x.offsetY) > 0.5f } == true }) scrollEpoch += 1
        val st = HashMap<Int, Offset>()
        for (x in out.sticky) st[x.index.toInt()] = Offset(x.dx, x.dy)
        if (st != sticky) sticky = st
        val ts = out.toasts.map { ToastInfo(it.id, it.durationMs, it.kind) }
        if (ts != toasts) toasts = ts
        if (out.direction != direction) direction = out.direction
        if (out.breakpoint != breakpoint) breakpoint = out.breakpoint
        if (passCount == 0 || out.visualChanges.isNotEmpty() || styles.size != nodes.size) {
            readVisuals()
            // The core resolves a leaf's text style DURING the pass (the
            // recipe's font); read it after, never from the pre-pass nodes.
            readTextStyles()
        }
        val size = Size(out.surfaceWidth, out.surfaceHeight)
        if (size != surfaceSize) surfaceSize = size
        stats = PassStats(out.layoutNs.toLong(), System.nanoTime() - t0, out.upcalls.toInt(), out.measureRounds.toInt(), measurer?.calls ?: 0, nodes.size)
        if (pinnedDirty) {
            pinnedDirty = false
            scrollPinned = nodes.filter { n -> styles.getOrNull(n.index)?.sticky == true || n.props["stickyHeaders"]?.bool == true || n.props["stickyHeader"]?.bool == true }.map { it.index }
        }
        layoutNeeded = false
        passCount += 1
        layersChanged()
        reportUnknowns()
        echoFields()
        syncToasts()
    }

    /**
     * The component owning part `id` (the facade's leaf carries no owner):
     * the node at `index` when it is that id, else the nearest node whose id
     * prefixes it (`table.cell.alice.0` → `table`).
     */
    internal fun ownerComponentOf(index: Int, id: String, part: String?): String? {
        nodes.getOrNull(index)?.takeIf { it.id == id }?.let { return it.ownerComponent }
        var cut = id.lastIndexOf('.')
        while (cut > 0) {
            val prefix = id.substring(0, cut)
            byId[prefix]?.let { i -> nodes.getOrNull(i)?.let { n -> return n.ownerComponent ?: n.component } }
            cut = id.lastIndexOf('.', cut - 1)
        }
        return null
    }

    /** What the user typed into host-owned fields (not yet in the props): the measurer sizes autosize fields with it. */
    private fun liveTexts(): Map<String, String> {
        if (fieldsMap.isEmpty()) return emptyMap()
        val m = HashMap<String, String>()
        for ((id, f) in fieldsMap) if (f.focused || f.pendingChange) m[id] = f.text
        return m
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
        pinnedDirty = true
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
        pinnedDirty = true
        val fallback = effectiveTheme?.ink(mode) ?: if (mode == Mode.Dark) Color.White else Color.Black
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

    /** Is `index` the node `ancestor` or inside it? */
    internal fun isWithin(index: Int, ancestor: Int): Boolean {
        var i: Int? = index
        var hops = 0
        while (i != null && hops < 4096) {
            if (i == ancestor) return true
            i = nodes.getOrNull(i)?.parent
            hops += 1
        }
        return false
    }

    /**
     * Must a scroll of container `index` lay out again? Only when a windowed
     * list windows against it (its own window or one inside it) or a pinned
     * node sits in it; a plain container only moves paint, which its view
     * already did.
     */
    internal fun scrollNeedsPass(index: Int): Boolean =
        lists.values.any { it.windowed && isWithin(it.node, index) } || scrollPinned.any { isWithin(it, index) }

    /** The index of the node `id`. */
    fun indexOf(id: String): Int? = byId[id]

    /** The node at `index`. */
    fun node(index: Int): NodeInfo? = nodes.getOrNull(index)

    /** The frame (dp, surface coordinates) of `index`. */
    fun frame(index: Int): Rect = frames.getOrNull(index) ?: Rect.Zero

    /** The core's resolved box style of `index`. */
    fun style(index: Int): PaintStyle = styles.getOrNull(index) ?: PaintStyle.EMPTY

    /** The text colour of `index`. */
    fun ink(index: Int): Color = inks.getOrNull(index) ?: (effectiveTheme?.ink(mode) ?: if (mode == Mode.Dark) Color.White else Color.Black)

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
        effectiveTheme?.part(component, part, props, states, mode) ?: PartStyle.EMPTY

    /** A colour token. */
    fun color(name: String): Color? = effectiveTheme?.color(name, mode)

    /** A spacing token (geometry fallbacks without a theme). */
    fun spacing(name: String): Float = effectiveTheme?.spacing(name) ?: GeometrySpacing.value(name)

    /** A control size token. */
    fun control(name: String, fallback: Float): Float = effectiveTheme?.control(name, fallback) ?: fallback

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
            // The core paints the trigger recipe (open while ITS layer is);
            // the native picker popup adds `open` here.
            n.isPickerTrigger && popup == n.id && !n.open ->
                part(rc, "trigger", ownerProps(index), states + "open").style
            else -> base
        }
    }

    /** Does a Select paint its popup (recipe `native: false`)? The native menu is the default. */
    fun selectPainted(ownerProps: Props): Boolean = part("Select", "content", ownerProps).native == false

    /**
     * Does this picker trigger open a PLATFORM picker (native overlays: the
     * M3 menu of a Select whose recipe does not paint it, the M3 date
     * dialog)? Otherwise the core's popup layer opens.
     */
    fun nativePicker(n: NodeInfo): Boolean {
        if (options.overlays != OverlayPresentation.Native || !n.isPickerTrigger) return false
        return when (n.component) {
            "DatePicker" -> true
            "Select" -> !selectPainted(ownerProps(n.index))
            else -> false
        }
    }

    /** Is `layer` painted inside the surface (tooltips and toasts; every layer under [OverlayPresentation.Painted])? */
    fun paintsInSurface(layer: LayerInfo): Boolean {
        if (options.overlays == OverlayPresentation.Painted) return true
        return layer.kind == "Tooltip" || layer.isToast
    }

    /** The family of a theme family NAME through the host and the registry (null = default). */
    fun fontFamily(name: String?): FontFamily? = fontResolver.family(name)

    /** [shaper] with this surface's default family (the theme's `sans`): every measure and paint shapes through it. */
    fun textShaper(): TextShaper = shaper().also { it.defaultFamily = effectiveTheme?.sansFamily }

    companion object {
        /** Viewport growth (dp) that applies at once ([autoViewportHeight]). */
        const val VIEWPORT_STEP = 48f

        /** How long the visible height must rest before a smaller change applies (ms). */
        const val VIEWPORT_SETTLE_MS = 150L

        private fun roundHalfAway(x: Double): Double = if (x >= 0) kotlin.math.floor(x + 0.5) else -kotlin.math.floor(-x + 0.5)

        /** `v` snapped to `min + k·step` and clamped. */
        fun snap(v: Double, min: Double, max: Double, step: Double): Double {
            var x = if (step > 0) min + roundHalfAway((v - min) / step) * step else v
            x = kotlin.math.min(kotlin.math.max(x, kotlin.math.min(min, max)), kotlin.math.max(min, max))
            return roundHalfAway(x * 1e9) / 1e9
        }
    }
}
