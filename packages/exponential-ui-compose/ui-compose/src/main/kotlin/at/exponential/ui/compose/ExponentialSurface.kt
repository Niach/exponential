package at.exponential.ui.compose

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.Insets
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceEnvironment
import at.exponential.ui.model.setSurfaceScroll
import kotlinx.coroutines.launch
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.primitives.LocalPrimitiveTokens

/** The surface a node view paints (provided by [ExponentialSurface]). */
val LocalSurfaceModel = staticCompositionLocalOf<SurfaceModel> {
    error("LocalSurfaceModel is provided by ExponentialSurface")
}

/**
 * The Compose face of a surface: the root node at the core's frames, as
 * wide as its container and as tall as its content (wrap it in YOUR
 * scroller; nothing here scrolls the surface or uses a lazy list), with the
 * open layers presented on top (painted ones inside the surface, native
 * ones as dialogs, sheets and popups). Coordinates are physical
 * (`LocalLayoutDirection` pinned to Ltr: the core already mirrored an RTL
 * surface), sizes are the theme's (`fontScale` pinned to 1), the text
 * measurer the model's passes use is created here (one `TextMeasurer` on
 * that density + the model's font resolver, so measured == painted), the
 * generic primitives read the theme's tokens ([LocalPrimitiveTokens]). The
 * width the surface gets is reported to the model after layout
 * (`setViewport`), never during composition.
 */
@Composable
fun ExponentialSurface(model: SurfaceModel, modifier: Modifier = Modifier) {
    val outer = LocalDensity.current
    val density = remember(outer.density) { Density(outer.density, fontScale = 1f) }
    CompositionLocalProvider(
        LocalSurfaceModel provides model,
        LocalLayoutDirection provides LayoutDirection.Ltr,
        LocalDensity provides density,
        LocalPrimitiveTokens provides model.primitiveTokens,
    ) {
        val measurer = rememberTextMeasurer()
        val shaper = remember(measurer, density, model) { TextShaper(measurer, density.density, model.fontResolver) }
        DisposableEffect(model, shaper) {
            model.shaper = { shaper }
            model.invalidateMeasures()
            onDispose { }
        }
        SurfaceEnvironmentEffect(model, outer)
        SurfaceBody(model, density.density, modifier)
    }
}

/**
 * What the device says, into the model ([SurfaceEnvironment]): the locale
 * and time zone (the formatter), dark mode, the font scale (the core scales
 * the theme's type; painting stays at fontScale 1), the animator duration
 * scale (0 = reduced motion), a mouse (hover), the safe-drawing insets;
 * and the platform hooks: TalkBack announcements.
 */
@Composable
private fun SurfaceEnvironmentEffect(model: SurfaceModel, outer: Density) {
    val configuration = LocalConfiguration.current
    val context = LocalContext.current
    val view = LocalView.current
    val insets = WindowInsets.safeDrawing
    val layoutDirection = LocalLayoutDirection.current
    val top = insets.getTop(outer) / outer.density
    val bottom = insets.getBottom(outer) / outer.density
    val left = insets.getLeft(outer, layoutDirection) / outer.density
    val right = insets.getRight(outer, layoutDirection) / outer.density
    val locales = configuration.locales
    val tag = if (locales.isEmpty) "en-US" else locales[0].toLanguageTag()
    val dark = (configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK) == android.content.res.Configuration.UI_MODE_NIGHT_YES
    val animatorScale = remember(configuration) {
        runCatching { android.provider.Settings.Global.getFloat(context.contentResolver, android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f) }.getOrDefault(1f)
    }
    val highContrast = remember(configuration) { highTextContrast(context) }
    val mouse = remember(configuration) {
        (configuration.navigation == android.content.res.Configuration.NAVIGATION_TRACKBALL) ||
            android.view.InputDevice.getDeviceIds().any { id -> android.view.InputDevice.getDevice(id)?.supportsSource(android.view.InputDevice.SOURCE_MOUSE) == true }
    }
    val env = SurfaceEnvironment(
        locale = tag,
        timeZone = java.util.TimeZone.getDefault().id,
        systemDark = dark,
        systemHighContrast = highContrast,
        fontScale = outer.fontScale,
        reducedMotion = animatorScale == 0f,
        hover = mouse,
        insets = Insets(top, right, bottom, left),
    )
    SideEffect { model.setEnvironment(env) }
    DisposableEffect(model, view, context) {
        // ONE announcement event carrying the text; `assertive` first
        // interrupts what TalkBack is saying (the web's aria-live=assertive).
        val am = context.getSystemService(android.content.Context.ACCESSIBILITY_SERVICE) as? android.view.accessibility.AccessibilityManager
        model.announcer = { text, live ->
            if (live == "assertive" && am?.isEnabled == true) am.interrupt()
            @Suppress("DEPRECATION")
            view.announceForAccessibility(text)
        }
        onDispose { model.announcer = null }
    }
}

/** The platform's high-text-contrast flag (a hidden API on most versions: read reflectively, false when absent). */
private fun highTextContrast(context: android.content.Context): Boolean = runCatching {
    val am = context.getSystemService(android.content.Context.ACCESSIBILITY_SERVICE) as android.view.accessibility.AccessibilityManager
    am.javaClass.getMethod("isHighTextContrastEnabled").invoke(am) as Boolean
}.getOrDefault(false)

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SurfaceBody(model: SurfaceModel, density: Float, modifier: Modifier) {
    val height = model.surfaceSize.height
    val paintedModal = model.options.overlays == OverlayPresentation.Painted && model.layers.any { it.isModal }
    val bring = remember(model) { BringIntoViewRequester() }
    val scope = rememberCoroutineScope()
    DisposableEffect(model, bring) {
        model.scrollSurfaceHandler = { x, y ->
            val vh = if (model.viewportHeight > 0f) model.viewportHeight else model.surfaceSize.height
            scope.launch { bring.bringIntoView(Rect(x * density, y * density, (x + model.width) * density, (y + vh) * density)) }
        }
        onDispose { model.scrollSurfaceHandler = null }
    }
    Box(
        modifier
            .fillMaxWidth()
            .then(if (height > 0f) Modifier.height(height.dp) else Modifier)
            .onSizeChanged { model.setSurfaceWidth(it.width / density) }
            // The host's scroller: the visible part of the surface (unbounded
            // lists window against it, sticky pins follow it, round 2).
            .onGloballyPositioned { c ->
                val visible = c.boundsInWindow()
                val at = c.positionInWindow()
                val y = kotlin.math.max(0f, (visible.top - at.y) / density)
                val x = kotlin.math.max(0f, (visible.left - at.x) / density)
                if (visible.height > 0f) model.autoViewportHeight(visible.height / density)
                if (x != model.surfaceScroll.x || y != model.surfaceScroll.y) model.setSurfaceScroll(x, y)
            }
            .bringIntoViewRequester(bring)
            .surfaceKeys(model)
            .semantics { isTraversalGroup = true },
    ) {
        if (model.passCount > 0 && model.nodes.isNotEmpty()) {
            val frames = mapOf(0 to model.frame(0))
            FrameLayout(
                size = model.surfaceSize,
                frames = frames,
                modifier = Modifier
                    .wrapContentSize(Alignment.TopStart, unbounded = true)
                    // A painted modal blocks the rest for TalkBack.
                    .then(if (paintedModal) Modifier.hiddenFromAccessibility() else Modifier),
            ) {
                NodeView(0, Modifier.frameIndex(0))
            }
            PaintedLayers(model)
        }
        NativeOverlays(model)
        DatePopup(model)
    }
}

