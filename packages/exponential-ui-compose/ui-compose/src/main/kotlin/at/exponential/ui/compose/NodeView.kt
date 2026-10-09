package at.exponential.ui.compose

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onPlaced
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.zIndex
import androidx.compose.animation.core.withInfiniteAnimationFrameNanos
import at.exponential.ui.ffi.animationFrameJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.fire
import at.exponential.ui.model.focus
import at.exponential.ui.model.hover
import at.exponential.ui.model.press
import at.exponential.ui.model.pressDown
import at.exponential.ui.model.pressUp
import at.exponential.ui.model.resizeDrag
import at.exponential.ui.model.setOpen
import at.exponential.ui.model.toastHold
import at.exponential.ui.theme.AnimationSpec
import at.exponential.ui.theme.PaintStyle

/**
 * Which pressables handle their own gestures (no clickable wrapper): the
 * text fields, the picker triggers, the Slider track, Segmented controls, the
 * carousel indicator and Resizable handles.
 */
internal fun selfHandling(n: NodeInfo): Boolean {
    if (n.isTextField) return true
    return n.isPickerTrigger ||
        (n.component == "Slider" && n.part == "track") ||
        n.component == "Segmented" ||
        (n.component == "Box" && n.part == "indicator") ||
        (n.ownerComponent == "Resizable" && n.part == "handle")
}

/** One frame of a keyframe animation (round 2 §2): the channels the core computed. */
internal data class AnimFrame(val opacity: Float, val translateX: Float, val translateY: Float, val rotate: Float, val scale: Float, val band: Float?) {
    companion object {
        val REST = AnimFrame(1f, 0f, 0f, 0f, 1f, null)

        fun parse(json: String?): AnimFrame {
            val v = json?.let(JsonValue::parse) ?: return REST
            fun f(k: String, d: Float) = (v[k]?.number ?: d.toDouble()).toFloat()
            return AnimFrame(f("opacity", 1f), f("translateX", 0f), f("translateY", 0f), f("rotate", 0f), f("scale", 1f), v["band"]?.number?.toFloat())
        }
    }
}

/**
 * The live frame of a node's keyframe animation: from the node's entry
 * into the tree, one core frame (`animationFrameJson`) per display frame;
 * infinite ones through `withInfiniteAnimationFrameNanos` (tests and
 * animator scale 0 park them). Reduced motion = the rest frame, no loop.
 */
@Composable
private fun rememberAnimationFrame(id: String, spec: AnimationSpec?, reduced: Boolean): AnimFrame? {
    if (spec == null) return null
    val still = spec.reduced || reduced
    var frame by remember(id, spec.name, spec.timingJson) {
        mutableStateOf(AnimFrame.parse(runCatching { animationFrameJson(spec.name, spec.timingJson, if (still) 1e12 else 0.0, still) }.getOrNull()))
    }
    if (!still) {
        LaunchedEffect(id, spec.name, spec.timingJson) {
            val timing = JsonValue.parse(spec.timingJson)
            val duration = timing["durationMs"]?.number ?: 0.0
            val runs = timing["iterations"]?.number ?: 1.0
            var start = -1L
            while (true) {
                val now = if (spec.infinite) withInfiniteAnimationFrameNanos { it } else withFrameNanos { it }
                if (start < 0) start = now
                val elapsed = (now - start) / 1e6
                frame = AnimFrame.parse(runCatching { animationFrameJson(spec.name, spec.timingJson, elapsed, false) }.getOrNull())
                if (!spec.infinite && elapsed >= duration * runs) break
            }
        }
    }
    return frame
}

/**
 * One node of the surface, filling the constraints its parent
 * [FrameLayout] fixed at the core's frame: the box (background + gradient,
 * per-side borders, per-corner radii, shadows, opacity, transform, clip),
 * the leaf content ([LeafContent]) or the nested container layout, the
 * press handling (press down / up → the `:pressed` state through the
 * core, the tap → `press`, focus → `focus`), hover (a mouse), the
 * layer-trigger gestures, transitions (`transition` ms + its cubic
 * bezier) and keyframe animations, and the accessibility shape (OUTERMOST:
 * the node's traversal group). Hidden nodes emit nothing. [modifier]
 * carries the parent's `frameIndex` tag.
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
    val base = model.boxStyle(index)
    val size = frame.size
    val disabled = model.isDisabled(index)
    // `visibility: hidden` (inherited, as in CSS): paints nothing, takes no
    // taps and no focus.
    val invisible = LocalInvisible.current || base.visibilityHidden
    val inert = base.pointerEventsNone || invisible
    val pressable = node.pressable && !selfHandling(node) && !inert
    val tooltip = tooltipTarget(node, model)
    val style = transitioned(base, model.reducedMotion)
    val anim = rememberAnimationFrame(node.id, base.animation, model.reducedMotion)
    val focusRequester = remember(node.id) { FocusRequester() }
    val inputMode = LocalInputModeManager.current
    val request = model.focusRequest
    if (pressable || node.isFocusable) {
        LaunchedEffect(request) {
            if (request == node.id) {
                runCatching { focusRequester.requestFocus() }
                model.focusRequest = null
            }
        }
    }

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
                .focusRequester(focusRequester)
                .onFocusChanged { model.focus(node.id, it.isFocused, inputMode.inputMode == InputMode.Keyboard) }
                .combinedClickable(
                    interactionSource = interactions,
                    indication = null,
                    enabled = !disabled,
                    onLongClick = when {
                        tooltip != null -> ({ toggleTooltip(model, tooltip) })
                        else -> contextMenuOwner(node, model)?.let { owner -> { openContextMenu(model, owner) } }
                    },
                    onClick = { model.press(index) },
                )
        }
        // Self-handling controls (picker triggers, a slider track, carousel
        // dots) still take keyboard focus for their keys.
        node.pressable && !node.isTextField && selfHandling(node) && node.ownerComponent != "Resizable" && !invisible -> Modifier
            .focusRequester(focusRequester)
            .onFocusChanged { model.focus(node.id, it.isFocused, inputMode.inputMode == InputMode.Keyboard) }
            .focusable(enabled = !disabled)
        // A control without a handler still takes Tab focus (a web <button> does).
        node.isFocusable && !selfHandling(node) && !inert -> Modifier
            .focusRequester(focusRequester)
            .onFocusChanged { model.focus(node.id, it.isFocused, inputMode.inputMode == InputMode.Keyboard) }
            .focusable(enabled = !disabled)
            .then(if (tooltip != null) Modifier.pointerInput(tooltip) { detectTapGestures(onLongPress = { toggleTooltip(model, tooltip) }) } else Modifier)
        invisible -> Modifier
        tooltip != null -> Modifier.pointerInput(tooltip) {
            detectTapGestures(onLongPress = { toggleTooltip(model, tooltip) })
        }
        else -> contextMenuOwner(node, model)?.let { owner ->
            Modifier.pointerInput(owner) { detectTapGestures(onLongPress = { openContextMenu(model, owner) }) }
        } ?: Modifier
    }

    // A mouse over the node: its `hover` state (recipes' `hover`, `:hover`
    // styles, hover cards), on the nodes gpui tracks: pressables, nodes the
    // core marks `hoverStyled`, triggers, text fields and nodes with a
    // transition.
    val hoverable = (pressable || node.hoverStyled || node.triggerFor != null || node.isTextField || base.transitionMs != null) && !invisible
    val hover = if (hoverable) {
        Modifier.pointerInput(node.id) {
            awaitPointerEventScope {
                while (true) {
                    val e = awaitPointerEvent(PointerEventPass.Initial)
                    when (e.type) {
                        PointerEventType.Enter -> model.hover(node.id, true)
                        PointerEventType.Exit -> model.hover(node.id, false)
                        else -> {}
                    }
                }
            }
        }
    } else {
        Modifier
    }

    // A toast pauses its timer while pressed (touch) or hovered (a mouse):
    // two holds, so a click under a hovering mouse keeps it paused.
    val toast = if (node.ownerComponent == "Toast" && node.part == "root") {
        val owner = node.owner ?: node.id
        Modifier.pointerInput(owner) {
            awaitPointerEventScope {
                while (true) {
                    val e = awaitPointerEvent(PointerEventPass.Initial)
                    when (e.type) {
                        PointerEventType.Enter -> model.toastHold(owner, true, "hover")
                        PointerEventType.Exit -> model.toastHold(owner, false, "hover")
                        PointerEventType.Press -> model.toastHold(owner, true, "press")
                        PointerEventType.Release -> model.toastHold(owner, false, "press")
                        else -> {}
                    }
                }
            }
        }
    } else {
        Modifier
    }

    val clips = style.overflowHidden || style.overflowScroll || (style.rounded && !node.isLeaf)
    val clip = if (clips) Modifier.clip(BoxShape(style, size)) else Modifier
    val pinned = model.sticky.containsKey(index)
    val animated = anim?.let { a ->
        Modifier.graphicsLayer {
            alpha = a.opacity.coerceIn(0f, 1f)
            translationX = a.translateX * density
            translationY = a.translateY * density
            rotationZ = a.rotate
            scaleX = a.scale
            scaleY = a.scale
            transformOrigin = TransformOrigin.Center
        }
    } ?: Modifier
    val band = anim?.band?.let { b ->
        val color = model.color("background") ?: Color.White
        Modifier.drawWithContent {
            drawContent()
            val w = this.size.width
            val x0 = b * w
            drawRect(Brush.horizontalGradient(0f to color.copy(alpha = 0f), 0.5f to color.copy(alpha = 0.5f), 1f to color.copy(alpha = 0f), startX = x0, endX = x0 + w))
        }
    } ?: Modifier

    Box(
        modifier
            .then(if (pinned) Modifier.zIndex(1f) else Modifier)
            .traversal(index)
            .nodeSemantics(node, model)
            .then(animated)
            .then(hover)
            .then(toast)
            .then(press)
            .then(resizeHandle(node, model, !invisible) { inputMode.inputMode == InputMode.Keyboard })
            .paintedBox(style, size)
            .then(clip)
            .then(band),
    ) {
        // An overlay owner the core marks as a leaf still carries its inline
        // trigger (a press Menu's default button): paint the children.
        CompositionLocalProvider(LocalInvisible provides invisible) {
            if (node.isLeaf && model.children.getOrNull(index).isNullOrEmpty()) {
                LeafContent(LeafContext(model, node, size, style, model.ink(index), model.textStyle(index)))
            } else {
                ContainerContent(index, node, model, style, size)
            }
        }
        // An invisible box swallows the pointer over its whole area, so the
        // self-handling controls below (sliders, pickers) take nothing either.
        if (invisible && !LocalInvisible.current) Box(Modifier.matchParentSize().pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent().changes.forEach { it.consume() } } })
    }
}

/** Inside a `visibility: hidden` box (CSS inherits it): nothing below takes input. */
internal val LocalInvisible = compositionLocalOf { false }

/**
 * Round 1 transitions: the background, border colour and opacity of a node
 * whose visual sets `transition` animate to their new value over its ms
 * with its cubic bezier (none under reduced motion).
 */
@Composable
private fun transitioned(style: PaintStyle, reduced: Boolean): PaintStyle {
    val ms = style.transitionMs ?: return style
    if (ms <= 0f || reduced) return style
    val e = style.transitionEasing ?: listOf(0.25f, 0.1f, 0.25f, 1f)
    val spec = tween<Color>(ms.toInt(), easing = CubicBezierEasing(e[0], e[1], e[2], e[3]))
    val bg by animateColorAsState(style.background ?: Color.Transparent, spec, label = "bg")
    val border by animateColorAsState(style.borderColor ?: Color.Transparent, spec, label = "border")
    val opacity by animateFloatAsState(style.opacity ?: 1f, tween(ms.toInt(), easing = CubicBezierEasing(e[0], e[1], e[2], e[3])), label = "opacity")
    return style.copy(
        background = if (style.background == null && bg.alpha == 0f) null else bg,
        borderColor = if (style.borderColor == null && border.alpha == 0f) null else border,
        opacity = if (style.opacity == null && opacity == 1f) null else opacity,
    )
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

/** The context Menu (`openOn: contextmenu`) whose target contains this node (a long press opens it, round 1). */
private fun contextMenuOwner(node: NodeInfo, model: SurfaceModel): String? {
    var p = node.parent
    var hops = 0
    while (p != null && hops < 64) {
        val n = model.node(p) ?: return null
        if (n.isContextMenu) return n.id
        p = n.parent
        hops += 1
    }
    return null
}

/** Open a context Menu at its target (the core places the menu at the pointer it is given). */
private fun openContextMenu(model: SurfaceModel, owner: String) {
    val i = model.indexOf(owner) ?: return
    val f = model.frame(i)
    model.fire(i, "contextmenu", JsonValue.Obj(mapOf("x" to JsonValue.Num(f.center.x.toDouble()), "y" to JsonValue.Num(f.center.y.toDouble()))))
}

/**
 * A Resizable `handle` (round 2 §1): a drag along the group's axis from the
 * sizes at the drag START (`drag {phase, delta}` in dp of pointer travel),
 * on an 8 dp hit area centred on the hairline; focusable for its keys. The
 * travel is measured in ROOT coordinates: the handle itself moves with the
 * split after every `move`, so its local positions drift.
 */
@Composable
private fun resizeHandle(node: NodeInfo, model: SurfaceModel, enabled: Boolean, keyboard: () -> Boolean): Modifier {
    if (node.ownerComponent != "Resizable" || node.part != "handle") return Modifier
    // `orientation` = the LINE's: a horizontal line moves along y.
    val alongY = node.props["orientation"]?.string == "horizontal"
    val index = node.index
    val coords = remember(node.id) { arrayOfNulls<LayoutCoordinates>(1) }
    if (!enabled) return Modifier
    return Modifier
        .onPlaced { coords[0] = it }
        .focusable()
        .onFocusChanged { model.focus(node.id, it.isFocused, keyboard()) }
        .pointerInput(node.id, alongY) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                fun root(p: Offset): Offset = coords[0]?.takeIf { it.isAttached }?.localToRoot(p) ?: p
                val start = root(down.position)
                model.pressDown(node.id)
                model.resizeDrag(index, "start", 0f)
                down.consume()
                var last = 0f
                while (true) {
                    val e = awaitPointerEvent()
                    val c = e.changes.firstOrNull { it.id == down.id } ?: break
                    val at = root(c.position)
                    val delta = (if (alongY) at.y - start.y else at.x - start.x) / density
                    if (!c.pressed) {
                        model.resizeDrag(index, "end", delta)
                        break
                    }
                    if (delta != last) {
                        last = delta
                        model.resizeDrag(index, "move", delta)
                    }
                    c.consume()
                }
                model.pressUp(node.id)
            }
        }
}
