package at.exponential.ui.compose

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.withInfiniteAnimationFrameNanos
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.isAltPressed
import androidx.compose.ui.input.pointer.isBackPressed
import androidx.compose.ui.input.pointer.isCtrlPressed
import androidx.compose.ui.input.pointer.isForwardPressed
import androidx.compose.ui.input.pointer.isMetaPressed
import androidx.compose.ui.input.pointer.isShiftPressed
import androidx.compose.ui.input.pointer.isTertiaryPressed
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.input.pointer.util.addPointerInputChange
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.zIndex
import at.exponential.ui.ffi.animationFrameJson
import at.exponential.ui.ffi.swipeLock
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.boxHover
import at.exponential.ui.model.fire
import at.exponential.ui.model.isStripTab
import at.exponential.ui.model.isSwipeableTabContent
import at.exponential.ui.model.swipeBlockedAt
import at.exponential.ui.model.tabSwipe
import at.exponential.ui.model.tabDrag
import at.exponential.ui.model.focus
import at.exponential.ui.model.holdInput
import at.exponential.ui.model.holdPointer
import at.exponential.ui.model.hover
import at.exponential.ui.model.isHeld
import at.exponential.ui.model.isHoldButton
import at.exponential.ui.model.isKeyRecorderField
import at.exponential.ui.model.keyRecorderBlur
import at.exponential.ui.model.keyRecorderMouse
import at.exponential.ui.model.keyRecorderPress
import at.exponential.ui.model.keyRecorderRecording
import at.exponential.ui.model.press
import at.exponential.ui.model.pressDown
import at.exponential.ui.model.pressUp
import at.exponential.ui.model.resizeDrag
import at.exponential.ui.model.setOpen
import at.exponential.ui.model.toastHold
import at.exponential.ui.theme.AnimationSpec
import at.exponential.ui.theme.PaintStyle
import at.exponential.ui.theme.TransformOp

/**
 * Which pressables handle their own gestures (no clickable wrapper): the
 * text fields, the picker triggers, the Slider track, Segmented controls, the
 * carousel indicator and Resizable handles. Round 8: a Canvas (its
 * `pointer` leaf sends `press {x, y}` itself; never a focus stop or a role).
 */
internal fun selfHandling(n: NodeInfo): Boolean {
    if (n.isTextField) return true
    return (n.isPickerTrigger && !n.isSelectRowTrigger) ||
        (n.component == "Slider" && n.part == "track") ||
        n.component == "Segmented" ||
        (n.component == "Box" && n.part == "indicator") ||
        (n.ownerComponent == "Resizable" && n.part == "handle") ||
        n.component == "Canvas"
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
    // The host's paint-only hook (SurfaceModel.nodeModifier), outermost.
    val extra = model.nodeModifier?.invoke(index)
    NodeBody(index, node, model, if (extra != null) modifier.then(extra) else modifier)
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
    val style = transitionedTranslate(transitioned(base, model.reducedMotion), model.reducedMotion)
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

    // EXP-1259: a pressed Button scales to the theme's `scale.press` (none
    // under reduced motion).
    val pressSource = remember(node.id) { MutableInteractionSource() }
    val pressScale = if (pressable && node.component == "Button" && !model.reducedMotion) model.theme?.scale("press") ?: 1f else 1f
    val isPressed by pressSource.collectIsPressedAsState()
    // VAPP-162: a `hold` Button's `pressed` is the CORE's state (Surface.hold), never the painter's.
    val hold = pressable && node.isHoldButton()
    val holdPressed = hold && node.states.contains("pressed")
    val scale by animateFloatAsState(if (isPressed || holdPressed) pressScale else 1f, tween((model.theme?.motion("fast") ?: 120f).toInt(), easing = CubicBezierEasing(0.2f, 0f, 0f, 1f)), label = "press")
    val pressedScale = if (pressScale != 1f) Modifier.graphicsLayer { scaleX = scale; scaleY = scale } else Modifier

    val press: Modifier = when {
        hold -> holdGesture(index, node, model, disabled, focusRequester) { inputMode.inputMode == InputMode.Keyboard }
        pressable && node.isKeyRecorderField -> keyRecorderGesture(index, node, model, disabled, focusRequester) { inputMode.inputMode == InputMode.Keyboard }
        pressable -> {
            val interactions = pressSource
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
        node.pressable && !node.isTextField && selfHandling(node) && node.ownerComponent != "Resizable" && node.component != "Canvas" && !invisible -> Modifier
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

    // #EDE-65: the pointer entering a Box fires its `on.hover` (like a
    // Motion's `MotionDriver.hover`); leaving fires nothing.
    val boxEnter = if (node.component == "Box" && node.part == null && !inert) {
        Modifier.pointerInput("${node.id}.boxHover") {
            awaitPointerEventScope {
                while (true) {
                    val e = awaitPointerEvent(PointerEventPass.Initial)
                    when (e.type) {
                        PointerEventType.Enter -> model.boxHover(node.id, true)
                        PointerEventType.Exit -> model.boxHover(node.id, false)
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
            .then(pressedScale)
            .skeletonPulse(node.component == "Skeleton" && node.isLeaf, model.reducedMotion)
            .then(hover)
            .then(boxEnter)
            .then(toast)
            .then(press)
            .then(resizeHandle(node, model, !invisible) { inputMode.inputMode == InputMode.Keyboard })
            .then(tabStripDrag(node, model, !invisible))
            .then(tabPagerSwipe(node, model, !invisible))
            .paintedBox(style, size)
            .then(clip)
            .then(band),
    ) {
        // An overlay owner the core marks as a leaf still carries its inline
        // trigger (a press Menu's default button): paint the children.
        CompositionLocalProvider(LocalInvisible provides invisible) {
            if (node.isLeaf && model.children.getOrNull(index).isNullOrEmpty()) {
                LeafContent(LeafContext(model, node, size, style, inheritedInk(model.ink(index)), model.textStyle(index).forPart(node.ownerComponent ?: node.macroName, node.part, node.id)))
            } else {
                ContainerContent(index, node, model, style, size)
            }
        }
        // #EXP-1629: a `row` Select's popup hangs off its VALUE part.
        if (node.ownerComponent == "Select" && (node.part == "value" || node.part == "placeholder")) {
            val trigger = node.parent?.let(model::node)
            if (trigger != null && trigger.isSelectRowTrigger && model.nativePicker(trigger)) {
                val t = trigger.index
                SelectPopup(LeafContext(model, trigger, model.frame(t).size, model.boxStyle(t), model.ink(t), model.textStyle(t)))
            }
        }
        // An invisible box swallows the pointer over its whole area, so the
        // self-handling controls below (sliders, pickers) take nothing either.
        if (invisible && !LocalInvisible.current) Box(Modifier.matchParentSize().pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent().changes.forEach { it.consume() } } })
    }
}

/**
 * VAPP-162 (round-5 contract §9): a `hold: true` Button's pointer, the
 * core's hold machine (`Surface.hold`) fed with every down / move / up of
 * the first pointer in dp from the button's top-left (the core applies the
 * 24 px slop and fires holdStart / holdEnd / holdCancel). The pointer is
 * CLAIMED on down (every change consumed), so a parent's scroll or a long
 * press never steals the hold; a gesture that ends without an up (the
 * platform's cancel) is a `pointercancel`. Focus loss cancels (`blur`);
 * Space / Enter / Escape go through the surface's keys ([holdKey]). A
 * press never fires `press`.
 */
@Composable
private fun holdGesture(index: Int, node: NodeInfo, model: SurfaceModel, disabled: Boolean, focusRequester: FocusRequester, keyboard: () -> Boolean): Modifier = Modifier
    .focusRequester(focusRequester)
    .onFocusChanged {
        model.focus(node.id, it.isFocused, keyboard())
        if (!it.isFocused && model.isHeld(node.id)) model.holdInput(index, "blur")
    }
    .focusable(enabled = !disabled)
    .pointerInput(node.id, disabled) {
        if (disabled) return@pointerInput
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            down.consume()
            runCatching { focusRequester.requestFocus() }
            model.holdPointer(index, "pointerdown", down.position.x / density, down.position.y / density)
            var ended = false
            try {
                while (true) {
                    val e = awaitPointerEvent()
                    val c = e.changes.firstOrNull { it.id == down.id }
                    if (c == null) {
                        ended = true
                        model.holdInput(index, "pointercancel")
                        break
                    }
                    if (!c.pressed) {
                        ended = true
                        c.consume()
                        model.holdPointer(index, "pointerup", c.position.x / density, c.position.y / density)
                        break
                    }
                    if (c.position != c.previousPosition) model.holdPointer(index, "pointermove", c.position.x / density, c.position.y / density)
                    c.consume()
                }
            } finally {
                if (!ended) model.holdInput(index, "pointercancel")
            }
        }
    }

/**
 * #EXP-1629 (interactions-round1.json keyRecorder): a KeyRecorder's field.
 * A tap (or Enter / Space through the surface's keys) presses it; when its
 * `recording` turns true it takes the focus (focusable in touch mode too,
 * so the hardware keys reach it), and while it records the surface's keys
 * hand EVERY key to the core as `record` ([surfaceKeys]), mouse buttons 3-5
 * over it are `record` {type: mouse} (consumed: no tap), and losing the
 * focus is `blur` (the core: `stop` + recording false). Idle, the core
 * drops record and blur; recording, it drops the press.
 */
@Composable
private fun keyRecorderGesture(index: Int, node: NodeInfo, model: SurfaceModel, disabled: Boolean, focusRequester: FocusRequester, keyboard: () -> Boolean): Modifier {
    val recording = node.keyRecorderRecording
    LaunchedEffect(node.id, recording) {
        if (recording) runCatching { focusRequester.requestFocus() }
    }
    return Modifier
        .focusRequester(focusRequester)
        .onFocusChanged {
            model.focus(node.id, it.isFocused, keyboard())
            if (!it.isFocused && model.keyRecorderRecording(index)) model.keyRecorderBlur(index)
        }
        .focusable(enabled = !disabled)
        .pointerInput("${node.id}.keyRecorderMouse", disabled) {
            if (disabled) return@pointerInput
            awaitPointerEventScope {
                var tertiary = false
                var back = false
                var forward = false
                while (true) {
                    val e = awaitPointerEvent(PointerEventPass.Initial)
                    val b = e.buttons
                    val button = when {
                        e.type != PointerEventType.Press -> null
                        b.isTertiaryPressed && !tertiary -> 3
                        b.isBackPressed && !back -> 4
                        b.isForwardPressed && !forward -> 5
                        else -> null
                    }
                    tertiary = b.isTertiaryPressed
                    back = b.isBackPressed
                    forward = b.isForwardPressed
                    if (button != null && model.keyRecorderRecording(index)) {
                        val m = e.keyboardModifiers
                        model.keyRecorderMouse(index, button, m.isCtrlPressed, m.isAltPressed, m.isShiftPressed, m.isMetaPressed)
                        e.changes.forEach { it.consume() }
                    }
                }
            }
        }
        .pointerInput("${node.id}.keyRecorderTap", disabled) {
            if (disabled) return@pointerInput
            detectTapGestures(
                onPress = {
                    model.pressDown(node.id)
                    tryAwaitRelease()
                    model.pressUp(node.id)
                },
                onTap = { model.keyRecorderPress(index) },
            )
        }
}

/**
 * NEW-77: an inherited ink swap (from → to): a VoiceWave paints its child
 * in the `VoiceWave/child` recipe colour, so every leaf below whose ink is
 * the wave's own (inherited, not set on the child) takes the recipe's.
 */
internal val LocalInkSwap = compositionLocalOf<Pair<Color, Color>?> { null }

/** [ink] after the enclosing [LocalInkSwap]. */
@Composable
internal fun inheritedInk(ink: Color): Color = LocalInkSwap.current?.let { (from, to) -> if (ink == from) to else ink } ?: ink

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

/**
 * FEED-200: a pure-translation `transform` animates to its new value over
 * the node's `transition` (CSS animates transforms too): the swipeable
 * Tabs' panels and `thumb` settle this way after a release. A drag sets
 * `transition: 0` (they follow the finger); no transition or reduced
 * motion = a snap. Only nodes with a transform or a transition track it.
 */
@Composable
private fun transitionedTranslate(style: PaintStyle, reduced: Boolean): PaintStyle {
    if (style.transform.isEmpty() && style.transitionMs == null) return style
    if (style.transform.any { it !is TransformOp.Translate }) return style
    var tx = 0f
    var ty = 0f
    for (op in style.transform) if (op is TransformOp.Translate) {
        tx += op.x
        ty += op.y
    }
    val ms = style.transitionMs ?: 0f
    val e = style.transitionEasing ?: listOf(0.25f, 0.1f, 0.25f, 1f)
    val spec = if (ms <= 0f || reduced) snap<Float>() else tween<Float>(ms.toInt(), easing = CubicBezierEasing(e[0], e[1], e[2], e[3]))
    val x by animateFloatAsState(tx, spec, label = "translateX")
    val y by animateFloatAsState(ty, spec, label = "translateY")
    if (x == tx && y == ty) return style
    return style.copy(transform = listOf(TransformOp.Translate(x, y)))
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
 * VAPP-279: a `strip` Tabs' tab drags into place after a long press (the
 * platform's long-press timeout, `tabLongPressMs` on the web): `drag
 * {phase, delta}` in dp along the row since the drag STARTED, in ROOT
 * coordinates (the tab moves with the core's preview after each `move`).
 */
@Composable
private fun tabStripDrag(node: NodeInfo, model: SurfaceModel, enabled: Boolean): Modifier {
    if (!enabled || !model.isStripTab(node.index)) return Modifier
    val index = node.index
    val coords = remember(node.id) { arrayOfNulls<LayoutCoordinates>(1) }
    return Modifier
        .onPlaced { coords[0] = it }
        .pointerInput(node.id) {
            fun root(p: Offset): Offset = coords[0]?.takeIf { it.isAttached }?.localToRoot(p) ?: p
            var start = Offset.Zero
            var travel = 0f
            detectDragGesturesAfterLongPress(
                onDragStart = { at ->
                    start = root(at)
                    travel = 0f
                    model.tabDrag(index, "start", 0f)
                },
                onDrag = { change, _ ->
                    travel = (root(change.position).x - start.x) / density
                    model.tabDrag(index, "move", travel)
                    change.consume()
                },
                onDragEnd = { model.tabDrag(index, "end", travel) },
                onDragCancel = { model.tabDrag(index, "end", 0f) },
            )
        }
}

/**
 * FEED-200 (fixtures/swipe.json): a `swipeable` Tabs' `content` pages its
 * panels by a horizontal TOUCH swipe (a mouse keeps its clicks and
 * selection). A touch starting inside a node that scrolls, pans or drags
 * sideways ([swipeBlockedAt]) is that node's. Otherwise the travel since
 * the down goes through the core's `swipeLock`: `pending` waits (nothing
 * consumed, so the panel's own presses still work), `vertical` lets go
 * (the page scrolls), `horizontal` claims the pointer: `start`, then a
 * `move` per change with the screen dp since the touch started, `end` with
 * the release velocity (dp/ms); a gesture that ends without an up (or a
 * child that consumed it first) is a `cancel`. Screen offsets: the core
 * mirrors under RTL.
 */
@Composable
private fun tabPagerSwipe(node: NodeInfo, model: SurfaceModel, enabled: Boolean): Modifier {
    if (!enabled || !model.isSwipeableTabContent(node.index)) return Modifier
    val index = node.index
    return Modifier.pointerInput(node.id) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            if (down.type != PointerType.Touch) return@awaitEachGesture
            if (model.swipeBlockedAt(index, down.position.x / density, down.position.y / density)) return@awaitEachGesture
            val tracker = VelocityTracker()
            tracker.addPointerInputChange(down)
            var locked = false
            var ended = false
            var offset = 0f
            try {
                while (true) {
                    val e = awaitPointerEvent()
                    val c = e.changes.firstOrNull { it.id == down.id } ?: break
                    tracker.addPointerInputChange(c)
                    val dx = (c.position.x - down.position.x) / density
                    val dy = (c.position.y - down.position.y) / density
                    if (!c.pressed) {
                        if (locked) {
                            c.consume()
                            ended = true
                            offset = dx
                            // px/s → dp/ms.
                            model.tabSwipe(index, "end", offset, tracker.calculateVelocity().x / density / 1000f)
                        }
                        break
                    }
                    if (!locked) {
                        // A child (a scroller of its own) took the move first.
                        if (c.isConsumed) break
                        when (runCatching { swipeLock(dx.toDouble(), dy.toDouble()) }.getOrDefault("pending")) {
                            "horizontal" -> {
                                locked = true
                                offset = dx
                                c.consume()
                                model.tabSwipe(index, "start", offset)
                            }
                            "vertical" -> break
                            else -> {}
                        }
                    } else {
                        if (c.positionChanged()) {
                            offset = dx
                            model.tabSwipe(index, "move", offset)
                        }
                        c.consume()
                    }
                }
            } finally {
                if (locked && !ended) model.tabSwipe(index, "cancel", offset)
            }
        }
    }
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
