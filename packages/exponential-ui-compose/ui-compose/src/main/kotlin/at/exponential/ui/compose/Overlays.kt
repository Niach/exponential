package at.exponential.ui.compose

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
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
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import at.exponential.ui.json.str
import at.exponential.ui.model.LayerInfo
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.dateValue
import at.exponential.ui.model.closeSubmenu
import at.exponential.ui.model.dismissLayer
import at.exponential.ui.model.isSubmenuLayer
import at.exponential.ui.model.modalLayers
import at.exponential.ui.model.pickDate
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
    for (layer in model.layers) {
        key(layer.owner, layer.root) {
            if (model.paintsInSurface(layer)) {
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
                LayerView(layer, Modifier.offset(layer.frame.left.dp, (layer.frame.top + shift).dp).then(unbounded))
            }
        }
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
            if (layer.kind == "Drawer") DrawerLayer(model, layer) else DialogLayer(model, layer)
        }
    }
    for (layer in model.layers) {
        if (layer.isModal || layer.isToast || layer.kind == "Tooltip") continue
        key(layer.owner, layer.root) { PopoverLayer(model, layer) }
    }
}

/** A Dialog: the layer tree at its size in a platform dialog window (back / outside tap dismiss unless pinned). */
@Composable
private fun DialogLayer(model: SurfaceModel, layer: LayerInfo) {
    val dismissible = model.dismissible(layer.owner)
    Dialog(
        onDismissRequest = { if (dismissible) model.dismissLayer(layer.owner) },
        properties = DialogProperties(
            dismissOnBackPress = dismissible,
            dismissOnClickOutside = dismissible,
            usePlatformDefaultWidth = false,
        ),
    ) {
        val bg = model.color("popover") ?: model.color("background") ?: Color.Transparent
        val r = model.boxStyle(layer.root).radius
        Box(Modifier.background(bg, RoundedCornerShape(r.dp))) {
            LayerView(layer)
        }
    }
}

/** A Drawer: the layer tree in a modal bottom sheet with a drag handle (swipe / scrim / back dismiss unless pinned). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DrawerLayer(model: SurfaceModel, layer: LayerInfo) {
    val dismissible = model.dismissible(layer.owner)
    val state = rememberModalBottomSheetState(
        skipPartiallyExpanded = true,
        confirmValueChange = { it != SheetValue.Hidden || dismissible },
    )
    ModalBottomSheet(
        onDismissRequest = { if (dismissible) model.dismissLayer(layer.owner) },
        sheetState = state,
        containerColor = model.color("background") ?: BottomSheetDefaults.ContainerColor,
        dragHandle = { BottomSheetDefaults.DragHandle() },
        properties = ModalBottomSheetProperties(shouldDismissOnBackPress = dismissible),
    ) {
        Box(Modifier.fillMaxWidth().padding(bottom = 12.dp), contentAlignment = Alignment.TopCenter) {
            LayerView(layer)
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
    val provider = remember(layer, density) { LayerPositionProvider(layer, density) }
    val submenu = model.isSubmenuLayer(layer)
    Popup(
        popupPositionProvider = provider,
        onDismissRequest = { if (submenu) model.closeSubmenu(layer.owner) else model.dismissLayer(layer.owner) },
        properties = PopupProperties(focusable = true),
    ) {
        LayerView(layer)
    }
}

/**
 * Positions a popup like the core placed the layer: the side it landed on
 * (`placementSide`, else `position`), the core's gap to the anchor and its
 * cross-axis offset, flipped to the other side when the window has no room
 * there and clamped into the window. `anchorBounds` = the surface (the
 * popup's parent), so surface coordinates map by its origin.
 */
private class LayerPositionProvider(val layer: LayerInfo, val density: Float) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val d = density
        val ox = anchorBounds.left.toFloat()
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
