package at.exponential.ui.compose

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.OverlayPresentation
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
        SurfaceBody(model, density.density, modifier)
    }
}

@Composable
private fun SurfaceBody(model: SurfaceModel, density: Float, modifier: Modifier) {
    val height = model.surfaceSize.height
    val paintedModal = model.options.overlays == OverlayPresentation.Painted && model.layers.any { it.isModal }
    Box(
        modifier
            .fillMaxWidth()
            .then(if (height > 0f) Modifier.height(height.dp) else Modifier)
            .onSizeChanged { model.setViewport(it.width / density, model.viewportHeight, model.maxHeight) }
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

