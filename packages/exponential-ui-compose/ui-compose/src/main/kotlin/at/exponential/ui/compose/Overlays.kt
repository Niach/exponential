package at.exponential.ui.compose

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.ModalBottomSheetProperties
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.AbsoluteAlignment
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import at.exponential.ui.json.str
import at.exponential.ui.model.LayerInfo
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.closeSubmenu
import at.exponential.ui.model.dateValue
import at.exponential.ui.model.dismissLayer
import at.exponential.ui.model.isMenuPreviewLayer
import at.exponential.ui.model.isSubmenuLayer
import at.exponential.ui.model.modalLayers
import at.exponential.ui.model.pickDate
import at.exponential.ui.model.popupTakesFocus
import at.exponential.ui.paint.DateModel
import kotlin.math.max
import kotlin.math.roundToInt

/** Is the overlay `owner` dismissible (`dismissible: false` pins it open)? */
private fun SurfaceModel.dismissible(owner: String): Boolean =
    indexOf(owner)?.let(::node)?.props?.get("dismissible")?.bool ?: true

/**
 * A layer's tree: the layer root at (0, 0) at the layer's size (dp), its
 * subtree nested at the core's frames.
 */
@Composable
internal fun LayerView(layer: LayerInfo, modifier: Modifier = Modifier) {
    val frames = remember(layer.root, layer.frame.width, layer.frame.height) {
        mapOf(layer.root to Rect(0f, 0f, layer.frame.width, layer.frame.height))
    }
    FrameLayout(size = layer.frame.size, frames = frames, modifier = modifier) {
        NodeView(layer.root, Modifier.frameIndex(layer.root))
    }
}

/**
 * VAPP-281 (round-7 contract): one anchored Popover / Menu layer the HOST
 * shows outside the surface: [anchor] = the anchor's frame in SURFACE
 * coordinates (dp), [side] + [align] the placement, [size] the popup's
 * unclipped size; [content] paints the layer at (0, 0) and routes presses,
 * keys and dismissals back to the surface.
 */
class HostedPopup(val layer: LayerInfo, val content: @Composable () -> Unit) {
    val owner: String get() = layer.owner
    val kind: String get() = layer.kind
    val anchor: Rect get() = layer.anchorFrame ?: layer.frame
    val side: String get() = layer.placementSide ?: layer.position
    val align: String get() = layer.align ?: "center"
    val size: androidx.compose.ui.geometry.Size get() = layer.frame.size
}

/**
 * VAPP-281: hosts the surface's hosted popups (a desktop's popup windows, a
 * compositor's popups): [Popups] composes inside the surface's composition
 * (so [HostedPopup.content] finds the model) and places each where it
 * wants; [dismiss] closes one (its window system dismissed it).
 */
interface PopupHost {
    @Composable
    fun Popups(popups: List<HostedPopup>, dismiss: (owner: String) -> Unit)
}

/** VAPP-281: the host of [LayerInfo.hosted] layers; null = they paint as the default popups. */
val LocalPopupHost = staticCompositionLocalOf<PopupHost?> { null }

/** VAPP-281: hand the hosted layers to [LocalPopupHost]. */
@Composable
internal fun HostedPopups(model: SurfaceModel) {
    val host = LocalPopupHost.current ?: return
    val popups = model.layers.filter { it.hosted }.map { layer -> HostedPopup(layer) { LayerView(layer) } }
    host.Popups(popups) { owner -> model.dismissLayer(owner) }
}

/**
 * Layers painted INSIDE the surface at the core's frames (surface
 * coordinates): tooltips always; every kind under
 * [OverlayPresentation.Painted], with a scrim for Dialog / Drawer (the
 * `Dialog/overlay` part's background; a tap dismisses unless
 * `dismissible: false`) and an outside-tap catcher for the rest.
 */
@Composable
internal fun PaintedLayers(model: SurfaceModel) {
    val catcherW = model.surfaceSize.width
    val catcherH = max(model.surfaceSize.height, model.viewportHeight)
    val hosting = LocalPopupHost.current != null
    for (layer in model.layers) {
        key(layer.owner, layer.root) {
            if (model.paintsInSurface(layer) && !(hosting && layer.hosted)) {
                // A viewport-placed layer (a centred dialog, an edge sheet, the
                // toasts) follows the part of the surface the host shows.
                val shift = if (layer.placementSide == null && layer.position != "point") model.surfaceScroll.y else 0f
                val unbounded = Modifier.wrapContentSize(Alignment.TopStart, unbounded = true)
                if (layer.isModal) {
                    val scrim = model.part("Dialog", "overlay", emptyMap()).style.background ?: Color.Black.copy(alpha = 0.5f)
                    Box(
                        unbounded
                            .requiredSize(catcherW.dp, catcherH.dp)
                            .background(scrim)
                            .pointerInput(layer.owner) {
                                detectTapGestures { if (model.dismissible(layer.owner)) model.dismissLayer(layer.owner) }
                            }
                            .hiddenFromAccessibility(),
                    )
                } else if (layer.kind != "Tooltip" && !layer.isToast && !model.isSubmenuLayer(layer)) {
                    // A submenu has no catcher of its own: the menu's catcher
                    // covers the outside, its parent rows stay pressable.
                    Box(
                        unbounded
                            .requiredSize(catcherW.dp, catcherH.dp)
                            .pointerInput(layer.owner) { detectTapGestures { model.dismissLayer(layer.owner) } }
                            .hiddenFromAccessibility(),
                    )
                }
                LayerView(layer, Modifier.offset(layer.frame.left.dp, (layer.frame.top + shift).dp).then(unbounded).layerEnter(model, layer))
            }
        }
    }
}

/**
 * EXP-1259 (the EXP-1243 rules): a painted overlay enters on the theme's
 * `motion.standard` (`decelerate`), scaling from `scale.popoverFrom`
 * toward its trigger's side while it fades in; a toast only fades; nothing
 * moves under reduced motion. (Closing is the core's: the layer leaves at
 * once.)
 */
@Composable
private fun Modifier.layerEnter(model: SurfaceModel, layer: LayerInfo): Modifier {
    val progress = remember(layer.owner, layer.root) { Animatable(if (model.reducedMotion) 1f else 0f) }
    LaunchedEffect(layer.owner, layer.root) {
        if (!model.reducedMotion) progress.animateTo(1f, tween((model.theme?.motion("standard") ?: 180f).toInt(), easing = CubicBezierEasing(0f, 0f, 0.2f, 1f)))
    }
    val from = if (layer.isToast) 1f else model.theme?.scale("popoverFrom") ?: 0.96f
    val originY = if (layer.position == "top") 1f else 0f
    return this.graphicsLayer {
        alpha = progress.value
        val s = from + (1f - from) * progress.value
        scaleX = s
        scaleY = s
        transformOrigin = TransformOrigin(0.5f, originY)
    }
}

/**
 * Native presentations of the open layers (under
 * [OverlayPresentation.Native]): a modal Drawer in an M3
 * `ModalBottomSheet`, any other modal (Dialog, AlertDialog) in a platform
 * `Dialog` window (a second modal stacks on the first), every other
 * overlay (Popover, a Menu and its open submenu, the core's picker popups)
 * in a `Popup` anchored at the core's anchor frame on the side it landed.
 * A Menu is the core's layer (round 3, as gpui and SwiftUI): its rows,
 * checkbox rows with the `Menu.check` glyph, the submenu indicator, and an
 * open submenu as its own layer beside its row. Tooltips and toasts are
 * painted ([PaintedLayers]).
 */
@Composable
internal fun NativeOverlays(model: SurfaceModel) {
    if (model.options.overlays != OverlayPresentation.Native) return
    for (layer in model.modalLayers) {
        key(layer.owner) {
            when {
                layer.kind != "Drawer" -> DialogLayer(model, layer)
                layer.position == "left" || layer.position == "right" || layer.position == "top" -> SideDrawerLayer(model, layer)
                else -> DrawerLayer(model, layer)
            }
        }
    }
    val hosting = LocalPopupHost.current != null
    for (layer in model.layers) {
        if (layer.isModal || layer.isToast || layer.kind == "Tooltip" || (hosting && layer.hosted)) continue
        key(layer.owner, layer.root) { PopoverLayer(model, layer) }
    }
}

/** A Dialog: the layer tree at its size in a platform dialog window (back / outside tap dismiss unless pinned). */
@Composable
private fun DialogLayer(model: SurfaceModel, layer: LayerInfo) {
    val dismissible = model.dismissible(layer.owner)
    // #VAPP-252: the core centres it in the window less the insets (the keyboard's
    // among them), the safe area its dialog window sits in; the body scrolls only
    // if it truly doesn't fit. Asked of the HOST's view: the dialog window hugs
    // the Dialog, a height read inside it would shrink it pass by pass.
    val host = LocalView.current
    val density = LocalDensity.current.density
    DisposableEffect(model, host, density) {
        val follow = { if (host.height > 0) model.overlayViewportHeight(host.height / density) }
        val listener = android.view.View.OnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> follow() }
        host.addOnLayoutChangeListener(listener)
        follow()
        onDispose {
            host.removeOnLayoutChangeListener(listener)
            model.overlayViewportHeight(null)
        }
    }
    // EXP-1676: the height it opened at (this composable is keyed by the
    // owner, so a reopened Dialog starts over): from then on it holds its
    // bottom edge, its content growing or shrinking upward (the core's rule).
    val held = remember { floatArrayOf(0f) }
    if (held[0] <= 0f && layer.frame.height > 0f) held[0] = layer.frame.height
    Dialog(
        onDismissRequest = { if (dismissible) model.dismissLayer(layer.owner) },
        properties = DialogProperties(
            dismissOnBackPress = dismissible,
            dismissOnClickOutside = dismissible,
            usePlatformDefaultWidth = false,
        ),
    ) {
        // The platform window stays centred (gravity CENTER); its y offset
        // moves it up by half of what it grew, so its bottom edge holds.
        val dialogWindow = (LocalView.current.parent as? DialogWindowProvider)?.window
        val y = dialogHoldOffset(layer.frame.height, held[0], density, host.height / density)
        SideEffect {
            dialogWindow?.let { w ->
                val attrs = w.attributes
                if (attrs.y != y) {
                    attrs.y = y
                    w.attributes = attrs
                }
            }
        }
        val bg = model.color("popover") ?: model.color("background") ?: Color.Transparent
        val r = model.boxStyle(layer.root).radius
        Box(Modifier.background(bg, RoundedCornerShape(r.dp))) {
            LayerView(layer)
        }
    }
}

/**
 * EXP-1676: a centred dialog window's y offset (px, gravity CENTER) that
 * holds its bottom edge: up by half of what it grew past `heldDp` (down by
 * half of what it shrank), never past `OVERLAY_PADDING` below the top of a
 * `frameDp`-tall frame (0 = unknown: no clamp).
 */
internal fun dialogHoldOffset(heightDp: Float, heldDp: Float, density: Float, frameDp: Float): Int {
    if (heldDp <= 0f) return 0
    var offset = -(heightDp - heldDp) / 2f
    if (frameDp > 0f) offset = max(offset, 8f - (frameDp - heightDp) / 2f)
    return (offset * density).roundToInt()
}

/**
 * A Drawer: the layer tree in a modal bottom sheet (swipe / scrim / back
 * dismiss unless pinned). The layer paints the ONE sheet itself (its
 * `content` surface, top radius and `handle` grabber, like every other
 * renderer), so the M3 sheet is only the window, the gesture and the scrim:
 * no container, no shape, no elevation, no drag handle of its own.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DrawerLayer(model: SurfaceModel, layer: LayerInfo) {
    val dismissible = model.dismissible(layer.owner)
    val state = rememberModalBottomSheetState(
        skipPartiallyExpanded = true,
        confirmValueChange = { it != SheetValue.Hidden || dismissible },
    )
    val scrim = model.part("Dialog", "overlay", emptyMap()).style.background ?: BottomSheetDefaults.ScrimColor
    ModalBottomSheet(
        onDismissRequest = { if (dismissible) model.dismissLayer(layer.owner) },
        sheetState = state,
        shape = RectangleShape,
        containerColor = Color.Transparent,
        tonalElevation = 0.dp,
        scrimColor = scrim,
        dragHandle = null,
        // #VAPP-253: the sheet spans the window, a tablet's too; the core keeps
        // its content clear of the safe area.
        sheetMaxWidth = Dp.Unspecified,
        contentWindowInsets = { WindowInsets(0, 0, 0, 0) },
        properties = ModalBottomSheetProperties(shouldDismissOnBackPress = dismissible),
    ) {
        BoxWithConstraints(Modifier.fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
            val windowWidth = maxWidth.value
            val windowHeight = maxHeight.value
            DisposableEffect(model, windowWidth, windowHeight) {
                model.overlayLayerWidth(windowWidth)
                // #EXP-1694: the sheet's height cap is the window's, not a short page's.
                model.overlayViewportHeight(windowHeight)
                onDispose {
                    model.overlayLayerWidth(null)
                    model.overlayViewportHeight(null)
                }
            }
            LayerView(layer)
        }
    }
}

/**
 * A Drawer on a side edge or the top: a full-screen dialog window holding the scrim and,
 * against that edge, the layer tree at the core's size, the window's height
 * (back / a scrim tap dismiss unless pinned). A bottom sheet is [DrawerLayer]'s.
 */
@Composable
private fun SideDrawerLayer(model: SurfaceModel, layer: LayerInfo) {
    val dismissible = model.dismissible(layer.owner)
    val scrim = model.part("Dialog", "overlay", emptyMap()).style.background ?: Color.Black.copy(alpha = 0.5f)
    Dialog(
        onDismissRequest = { if (dismissible) model.dismissLayer(layer.owner) },
        properties = DialogProperties(
            dismissOnBackPress = dismissible,
            dismissOnClickOutside = false,
            usePlatformDefaultWidth = false,
            decorFitsSystemWindows = false,
        ),
    ) {
        // The scrim below is the core's colour; the window adds none of its own.
        (LocalView.current.parent as? DialogWindowProvider)?.window?.setDimAmount(0f)
        Box(Modifier.fillMaxSize()) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(scrim)
                    .pointerInput(layer.owner) {
                        detectTapGestures { if (dismissible) model.dismissLayer(layer.owner) }
                    }
                    .hiddenFromAccessibility(),
            )
            val edge = if (layer.position == "right") AbsoluteAlignment.TopRight else AbsoluteAlignment.TopLeft
            // The core sizes the panel to the viewport, and a surface that hugs
            // its content has a short one: hand it the window while this is up.
            BoxWithConstraints(Modifier.fillMaxSize()) {
                val windowHeight = maxHeight.value
                // #EXP-1694: a top Drawer spans the window, like the bottom sheet.
                val windowWidth = if (layer.position == "top") maxWidth.value else null
                DisposableEffect(model, windowHeight, windowWidth) {
                    model.overlayViewportHeight(windowHeight)
                    model.overlayLayerWidth(windowWidth)
                    onDispose {
                        model.overlayViewportHeight(null)
                        model.overlayLayerWidth(null)
                    }
                }
                Box(Modifier.align(edge)) {
                    LayerView(layer)
                }
            }
        }
    }
}

/**
 * A Popover, a Menu (or its open submenu), a picker popup: the layer tree
 * in a focusable popup at the anchor. An outside tap on a submenu closes
 * only the submenu (its row toggles it), on anything else the owner.
 */
@Composable
private fun PopoverLayer(model: SurfaceModel, layer: LayerInfo) {
    val density = LocalDensity.current.density
    val inWindow = model.layerViewportWidth != null
    val provider = remember(layer, density, inWindow) { LayerPositionProvider(layer, density, inWindow) }
    val submenu = model.isSubmenuLayer(layer)
    // #EXP-1629: a Menu item's preview never takes the focus (the menu
    // keeps its keys) and an outside tap is the menu's to handle.
    val preview = model.isMenuPreviewLayer(layer)
    Popup(
        popupPositionProvider = provider,
        onDismissRequest = { if (preview) Unit else if (submenu) model.closeSubmenu(layer.owner) else model.dismissLayer(layer.owner) },
        // #EDE-65: nor does a typeahead (a Popover anchored on a text
        // field): the field keeps the focus, its caret and the IME.
        properties = PopupProperties(focusable = model.popupTakesFocus(layer)),
    ) {
        LayerView(layer)
    }
}

/**
 * Positions a popup like the core placed the layer: the side it landed on
 * (`placementSide`, else `position`), the core's gap to the anchor and its
 * cross-axis offset, flipped to the other side when the window has no room
 * there and clamped into the window. `anchorBounds` = the surface (the
 * popup's parent), so surface coordinates map by its origin; #VAPP-253 above a
 * sheet spanning the window ([inWindow]) x is the window's already.
 */
private class LayerPositionProvider(val layer: LayerInfo, val density: Float, val inWindow: Boolean) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val d = density
        val ox = if (inWindow) 0f else anchorBounds.left.toFloat()
        val oy = anchorBounds.top.toFloat()
        val f = layer.frame
        val a = layer.anchorFrame ?: f
        val w = popupContentSize.width.toFloat()
        val h = popupContentSize.height.toFloat()
        var x = ox + f.left * d
        var y = oy + f.top * d
        when (layer.placementSide ?: layer.position) {
            "top" -> {
                val gap = max(0f, a.top - f.bottom) * d
                y = oy + a.top * d - gap - h
                if (y < 0f) y = oy + a.bottom * d + gap
            }
            "left" -> {
                val gap = max(0f, a.left - f.right) * d
                x = ox + a.left * d - gap - w
                if (x < 0f) x = ox + a.right * d + gap
            }
            "right" -> {
                val gap = max(0f, f.left - a.right) * d
                x = ox + a.right * d + gap
                if (x + w > windowSize.width) x = ox + a.left * d - gap - w
            }
            "bottom" -> {
                val gap = max(0f, f.top - a.bottom) * d
                y = oy + a.bottom * d + gap
                if (y + h > windowSize.height) y = oy + a.top * d - gap - h
            }
        }
        x = x.coerceIn(0f, max(0f, windowSize.width - w))
        y = y.coerceIn(0f, max(0f, windowSize.height - h))
        return IntOffset(x.roundToInt(), y.roundToInt())
    }
}

/**
 * The DatePicker popup: while `model.popup` names a DatePicker trigger, an
 * M3 `DatePickerDialog` (date only, UTC midnight millis ↔ ISO through
 * [DateModel], `min` / `max` from the owner's props as selectable dates).
 * Done → `pickDate`; dismiss / Cancel → `popup = null`.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun DatePopup(model: SurfaceModel) {
    val id = model.popup ?: return
    val fieldIndex = model.indexOf(id) ?: return
    val field = model.node(fieldIndex) ?: return
    if (field.component != "DatePicker") return
    key(id) {
        val props = model.ownerProps(fieldIndex)
        val min = DateModel.millis(props.str("min"))
        val max = DateModel.millis(props.str("max"))
        val selectable = remember(min, max) {
            object : SelectableDates {
                override fun isSelectableDate(utcTimeMillis: Long): Boolean =
                    (min == null || utcTimeMillis >= min) && (max == null || utcTimeMillis <= max)
            }
        }
        val state = rememberDatePickerState(
            initialSelectedDateMillis = DateModel.millis(model.dateValue(fieldIndex)),
            selectableDates = selectable,
        )
        DatePickerDialog(
            onDismissRequest = { model.popup = null },
            confirmButton = {
                TextButton(onClick = {
                    val ms = state.selectedDateMillis
                    if (ms != null) model.pickDate(fieldIndex, DateModel.iso(fromMillis = ms)) else model.popup = null
                }) { Text(model.string("confirm")) }
            },
            dismissButton = { TextButton(onClick = { model.popup = null }) { Text(model.string("cancel")) } },
        ) {
            DatePicker(state = state)
        }
    }
}
