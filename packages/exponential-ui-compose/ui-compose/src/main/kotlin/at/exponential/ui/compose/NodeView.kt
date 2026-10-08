package at.exponential.ui.compose

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.focus
import at.exponential.ui.model.press
import at.exponential.ui.model.pressDown
import at.exponential.ui.model.pressUp
import at.exponential.ui.model.setOpen
import kotlin.math.min

/**
 * Which pressables handle their own gestures (no clickable wrapper): the
 * text fields, the Select trigger, the Slider track, ToggleGroups and the
 * carousel indicator.
 */
internal fun selfHandling(n: NodeInfo): Boolean {
    if (n.isTextField) return true
    return (n.component == "Select" && n.part == "field") ||
        (n.component == "Slider" && n.part == "track") ||
        n.component == "ToggleGroup" ||
        (n.component == "Box" && n.part == "indicator")
}

/**
 * One node of the surface, filling the constraints its parent
 * [FrameLayout] fixed at the core's frame: the box (background, border,
 * radius, shadow, opacity, clip), the leaf content ([LeafContent]) or the
 * nested container layout, the press handling (press down / up → the
 * `:pressed` state through the core, the tap → `press`, keyboard focus →
 * `focus`), the layer-trigger gestures (Tooltip long press, the native
 * DropdownMenu anchored here) and the accessibility shape (OUTERMOST: the
 * node's traversal group). Hidden nodes emit nothing. [modifier] carries
 * the parent's `frameIndex` tag.
 */
@Composable
fun NodeView(index: Int, modifier: Modifier = Modifier) {
    val model = LocalSurfaceModel.current
    val node = model.node(index) ?: return
    if (node.hidden) return
    model.paintTrace?.add(index)
    NodeBody(index, node, model, modifier)
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun NodeBody(index: Int, node: NodeInfo, model: SurfaceModel, modifier: Modifier) {
    val frame = model.frame(index)
    val style = model.boxStyle(index)
    val size = frame.size
    val pressable = node.pressable && !selfHandling(node)
    val disabled = model.isDisabled(index)
    val tooltip = tooltipTarget(node, model)

    val press: Modifier = when {
        pressable -> {
            val interactions = remember(node.id) { MutableInteractionSource() }
            LaunchedEffect(interactions) {
                interactions.interactions.collect { i ->
                    when (i) {
                        is PressInteraction.Press -> model.pressDown(node.id)
                        is PressInteraction.Release, is PressInteraction.Cancel -> model.pressUp(node.id)
                    }
                }
            }
            Modifier
                .onFocusChanged { model.focus(node.id, it.isFocused) }
                .combinedClickable(
                    interactionSource = interactions,
                    indication = null,
                    enabled = !disabled,
                    onLongClick = tooltip?.let { t -> { toggleTooltip(model, t) } },
                    onClick = { model.press(index) },
                )
        }
        tooltip != null -> Modifier.pointerInput(tooltip) {
            detectTapGestures(onLongPress = { toggleTooltip(model, tooltip) })
        }
        else -> Modifier
    }

    val clips = style.overflowHidden || style.overflowScroll || (style.radius > 0f && !node.isLeaf)
    val clip = if (clips) {
        val r = min(style.radius, min(size.width, size.height) / 2f).coerceAtLeast(0f)
        Modifier.clip(RoundedCornerShape(r.dp))
    } else {
        Modifier
    }

    Box(
        modifier
            .traversal(index)
            .nodeSemantics(node, model)
            .then(press)
            .paintedBox(style, size)
            .then(clip),
    ) {
        // An overlay owner the core marks as a leaf still carries its inline
        // trigger (a DropdownMenu's default button): paint the children.
        if (node.isLeaf && model.children.getOrNull(index).isNullOrEmpty()) {
            LeafContent(LeafContext(model, node, size, style, model.ink(index), model.textStyle(index)))
        } else {
            ContainerContent(index, node, model, style, size)
        }
        TriggerMenu(node, model)
    }
}

/** The Tooltip a node triggers (its owner id), if any. */
private fun tooltipTarget(node: NodeInfo, model: SurfaceModel): String? {
    val target = node.triggerFor ?: return null
    val owner = model.indexOf(target)?.let(model::node) ?: return null
    return if (owner.component == "Tooltip") target else null
}

/** A long press on a Tooltip trigger toggles it (touch has no hover). */
private fun toggleTooltip(model: SurfaceModel, target: String) {
    model.setOpen(target, model.layers.none { it.owner == target })
}
