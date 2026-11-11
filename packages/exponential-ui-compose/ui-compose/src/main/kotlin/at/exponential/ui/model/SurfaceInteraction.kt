package at.exponential.ui.model

import at.exponential.ui.ffi.FfiEvent
import at.exponential.ui.ffi.UiException
import at.exponential.ui.ffi.swipeBlocks
import at.exponential.ui.host.FilePickRequest
import at.exponential.ui.host.InputKind
import at.exponential.ui.host.PickedFile
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.host.SurfaceFunctionCallEvent
import at.exponential.ui.host.SurfaceInputEvent
import at.exponential.ui.host.UploadEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.flag
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

// Interaction: presses, the OutEvent dispatch to the host, interaction
// states, mirrors of unbound controls, overlay bookkeeping, sliders, toggle
// groups, carousel pages and date picks. Every path ends in a layout pass
// (which re-reads the nodes: the core changes props / hidden without a
// structure-version bump) so the painted state follows the core's.

/**
 * Forward the core's OutEvents to the host, then re-read the nodes and lay
 * out. `inputRevision` = the revision of a host-owned text edit (else a
 * per-component counter).
 */
internal fun SurfaceModel.dispatch(events: List<FfiEvent>, inputRevision: Int? = null) {
    fun action(v: JsonValue) = SurfaceActionEvent(
        surfaceId = id,
        event = v["event"]?.string ?: "",
        name = v["name"]?.string ?: "",
        componentId = v["componentId"]?.string ?: "",
        context = v["context"] ?: JsonValue.Obj(emptyMap()),
        payload = v["payload"],
    )
    var next = 0
    while (next < events.size) {
        val e = events[next++]
        val v = JsonValue.parse(e.json)
        when (e.kind) {
            "action" -> host.onAction(action(v))
            "openUrl" -> openHref(v["url"]?.string ?: "")
            "functionCall" -> {
                val call = SurfaceFunctionCallEvent(
                    surfaceId = id,
                    componentId = v["componentId"]?.string ?: "",
                    name = v["name"]?.string ?: "",
                    args = v["args"]?.obj ?: emptyMap(),
                )
                // Round 9 §6.8: the function first, its event only when it did not fail.
                val held = events.getOrNull(next)?.takeIf { it.kind == "action" }
                if (held == null) {
                    host.onFunctionCall(call)
                } else {
                    next++
                    val plugin = host
                    val event = action(JsonValue.parse(held.json))
                    plugin.onFunctionCallThen(call) { plugin.onAction(event) }
                }
            }
            "input" -> {
                val component = v["componentId"]?.string ?: ""
                val revision = inputRevision ?: ((revisions[component] ?: 0) + 1).also { revisions[component] = it }
                host.onInput(
                    SurfaceInputEvent(
                        surfaceId = id,
                        componentId = component,
                        name = v["name"]?.string ?: "",
                        path = v["path"]?.string,
                        value = v["value"] ?: JsonValue.Null,
                        revision = revision,
                        kind = if (v["commit"]?.bool == true) InputKind.Commit else InputKind.Change,
                    ),
                )
            }
            "focus" -> focusRequest = v["id"]?.string
            "announce" -> announce(v["text"]?.string ?: "", v["live"]?.string ?: "polite")
            "copy" -> host.copy(v["text"]?.string ?: "")
            "pickFiles" -> host.pickFiles(
                FilePickRequest(this, v["componentId"]?.string ?: "", v["accept"]?.string, v["multiple"]?.bool ?: false),
            )
            "hoverTimer" -> {
                val owner = v["owner"]?.string ?: ""
                val delayMs = (v["delayMs"]?.number ?: 0.0).toLong()
                hoverTimers.remove(owner)?.cancel()
                hoverTimers[owner] = scope.launch {
                    delay(delayMs)
                    hoverTimers.remove(owner)
                    dispatch(surface.hoverTimeout(owner))
                }
            }
            "scrollSurface" -> scrollSurfaceHandler?.invoke((v["x"]?.number ?: 0.0).toFloat(), (v["y"]?.number ?: 0.0).toFloat())
            else -> {}
        }
    }
    nodesDirty = true
    layoutNeeded = true
    pass()
}

/** Speak `text` through the platform's live region (`polite | assertive`). */
fun SurfaceModel.announce(text: String, live: String = "polite") {
    if (text.isEmpty()) return
    announcer?.invoke(text, live)
}

/**
 * A host → surface command (`catalog/a11y.json` `commands`): `{focus: {id}}`,
 * `{announce: {text, live?}}`, `{scrollIntoView: {id}}`,
 * `{scrollToIndex: {id, index, align?}}`. Throws `UiException` when invalid.
 */
fun SurfaceModel.command(json: String) {
    dispatch(surface.commandJson(json))
}

/** Scroll list `id` so the item at DATA index `index` shows (`start | center | end | nearest`). */
fun SurfaceModel.scrollToIndex(id: String, index: Int, align: String? = null) {
    dispatch(surface.scrollToIndex(id, index.toUInt(), align))
}

/**
 * #EDE-65 R (interactions-round1.json composerPaste): a paste into Composer
 * [index] carrying [files] (with their content [uris], same order; read off
 * the clipboard, `composerClipboard`): the images (`image/`, clipboard
 * order) go to [HostPlugin.onUpload] (`name` = the Composer's id) and fire
 * `attach` {files: [{name, size, type}]}. `true` = taken (files present:
 * the field inserts nothing, also when none is an image); `false` = no
 * files, the field's own paste.
 */
fun SurfaceModel.composerPaste(index: Int, files: List<PickedFile>, uris: List<String> = emptyList()): Boolean {
    if (files.isEmpty()) return false
    val n = node(index) ?: return true
    if (n.component != "Composer") return true
    val picked = files.indices.filter { files[it].type.startsWith("image/") }
    if (picked.isEmpty()) return true
    val images = picked.map { files[it] }
    host.onUpload(UploadEvent(id, n.id, n.id, images, picked.mapNotNull { uris.getOrNull(it) }))
    val list = JsonValue.Arr(images.map { JsonValue.Obj(mapOf("name" to JsonValue.Str(it.name), "size" to JsonValue.Num(it.size.toDouble()), "type" to JsonValue.Str(it.type))) })
    fire(index, "attach", JsonValue.Obj(mapOf("files" to list)))
    return true
}

/** Files the host picked (or dropped) for FileUpload `componentId`: the surface gets `upload {files}` (name, size, type). */
fun SurfaceModel.filesPicked(componentId: String, files: List<PickedFile>) {
    val i = indexOf(componentId) ?: nodes.firstOrNull { it.owner == componentId && it.part == "dropzone" }?.index ?: return
    val list = JsonValue.Arr(files.map { JsonValue.Obj(mapOf("name" to JsonValue.Str(it.name), "size" to JsonValue.Num(it.size.toDouble()), "type" to JsonValue.Str(it.type))) })
    fire(i, "upload", JsonValue.Obj(mapOf("files" to list)))
}

/**
 * Start a timer for every new timed toast; forget the gone ones. A hold
 * ([toastHold]) pauses it; released, it runs the REST of its duration. The
 * timer ticks in 100 ms steps (a pause drops at most one partial step).
 */
internal fun SurfaceModel.syncToasts() {
    val live = toasts.map { it.id }.toSet()
    for (id in toastTimers.keys.filter { it !in live }) toastTimers.remove(id)?.cancel()
    toastHeld.keys.retainAll(live)
    toastLeft.keys.retainAll(live)
    for (t in toasts) {
        if (t.durationMs <= 0 || toastTimers.containsKey(t.id) || !toastHeld[t.id].isNullOrEmpty()) continue
        toastTimers[t.id] = scope.launch {
            while (true) {
                val left = toastLeft[t.id] ?: t.durationMs.toLong()
                if (left <= 0L) break
                val step = kotlin.math.min(left, 100L)
                delay(step)
                toastLeft[t.id] = left - step
            }
            toastTimers.remove(t.id)
            toastLeft.remove(t.id)
            dispatch(surface.dismissToast(t.id))
        }
    }
}

/**
 * A toast is held (`true`) or released by `reason` (`hover` a mouse over
 * it, `press` a finger or button down, `focus`): its timer pauses while ANY
 * hold lasts, then runs the rest of its duration.
 */
fun SurfaceModel.toastHold(id: String, held: Boolean, reason: String = "hover") {
    if (held) {
        toastHeld.getOrPut(id) { HashSet() }.add(reason)
        toastTimers.remove(id)?.cancel()
    } else {
        val holds = toastHeld[id] ?: return
        holds.remove(reason)
        if (holds.isEmpty()) {
            toastHeld.remove(id)
            syncToasts()
        }
    }
}

/**
 * Scroll container `id` (any `overflow: scroll | auto`, a windowed list) to
 * (x, y) dp. `fromView` = the container's own view scrolled (a fling): a
 * plain container then only records it (the view already moved the paint),
 * a windowed or pinning one lays out again. Any other caller (keys, the
 * host) lays out so the view follows the core's (clamped) offset.
 */
fun SurfaceModel.scrollTo(id: String, x: Float, y: Float, fromView: Boolean = false) {
    val index = indexOf(id)
    if (fromView && index != null) viewScroll[index] = androidx.compose.ui.geometry.Offset(x, y)
    if (!surface.scrollTo(id, x, y)) return
    if (fromView && index != null && !scrollNeedsPass(index)) return
    nodesDirty = nodesDirty || index == null || lists.values.any { it.windowed && isWithin(it.node, index) }
    layoutNeeded = true
    pass()
}

/** The host viewport's scroll of the whole surface (dp): unbounded lists window against it, sticky pins follow. */
fun SurfaceModel.setSurfaceScroll(x: Float, y: Float) {
    surfaceScroll = androidx.compose.ui.geometry.Offset(x, y)
    if (surface.setSurfaceScroll(x, y)) {
        nodesDirty = true
        layoutNeeded = true
        pass()
    }
}

/** A Resizable handle drag (`phase` = start | move | end, `delta` = px along the axis since the drag started). */
fun SurfaceModel.resizeDrag(index: Int, phase: String, delta: Float) {
    fire(index, "drag", JsonValue.Obj(mapOf("phase" to JsonValue.Str(phase), "delta" to JsonValue.Num(delta.toDouble()))))
}

/** VAPP-279: a `strip` Tabs' tab (it reorders by a long-press drag and by Ctrl+Shift+Arrow). */
fun SurfaceModel.isStripTab(index: Int): Boolean {
    val n = node(index) ?: return false
    if (n.ownerComponent != "Tabs" || n.part != "tab") return false
    val owner = n.owner?.let { indexOf(it) }?.let { node(it) } ?: return false
    return owner.props["variant"]?.string == "strip"
}

/** VAPP-279: a strip tab's reorder drag (`phase` = start | move | end, `delta` = dp along the row since the drag started, screen direction). */
fun SurfaceModel.tabDrag(index: Int, phase: String, delta: Float) {
    fire(index, "drag", JsonValue.Obj(mapOf("phase" to JsonValue.Str(phase), "delta" to JsonValue.Num(delta.toDouble()))))
}

/** FEED-200: the `content` part of a `swipeable` Tabs (its panels page by a horizontal touch swipe). */
fun SurfaceModel.isSwipeableTabContent(index: Int): Boolean {
    val n = node(index) ?: return false
    if (n.ownerComponent != "Tabs" || n.part != "content") return false
    return ownerProps(index)["swipeable"]?.bool == true
}

/**
 * FEED-200 (fixtures/swipe.json): a swipeable Tabs' pager swipe on its
 * `content` (`phase` = start | move | end | cancel, `offset` = screen dp
 * since the touch started, positive = rightwards; `velocity` = dp/ms at
 * the release). The core draws the panels and the `thumb` and settles.
 */
fun SurfaceModel.tabSwipe(index: Int, phase: String, offset: Float, velocity: Float = 0f) {
    fire(
        index,
        "swipe",
        JsonValue.Obj(linkedMapOf("phase" to JsonValue.Str(phase), "offset" to JsonValue.Num(offset.toDouble()), "velocity" to JsonValue.Num(velocity.toDouble()))),
    )
}

/**
 * FEED-200: is a touch at ([x], [y]) dp from the swipeable Tabs' `content`
 * (node [content]) the pager's? Not when it starts inside a node that
 * scrolls, pans or drags sideways (`swipeBlocks` of its component, its
 * `direction` prop and its resolved `overflowX`): the walk goes from the
 * content down through the topmost child whose frame holds the point
 * (skipping hidden and `pointerEvents: none` ones: the neighbour panels),
 * following the scroll offsets of the scrollers it passes.
 */
fun SurfaceModel.swipeBlockedAt(content: Int, x: Float, y: Float): Boolean {
    val origin = frame(content).topLeft
    var at = androidx.compose.ui.geometry.Offset(origin.x + x, origin.y + y)
    var i = content
    var hops = 0
    while (hops++ < 4096) {
        val hit = (children.getOrNull(i) ?: break).asReversed().firstOrNull { k ->
            val n = node(k)
            val s = style(k)
            n != null && !n.hidden && !s.pointerEventsNone && !s.visibilityHidden && frame(k).contains(at)
        } ?: break
        val n = node(hit) ?: break
        if (runCatching { swipeBlocks(n.component, n.props["direction"]?.string, style(hit).overflowX) }.getOrDefault(false)) return true
        scrolls[hit]?.let { at = androidx.compose.ui.geometry.Offset(at.x + it.offsetX, at.y + it.offsetY) }
        i = hit
    }
    return false
}

/** A key on a node the core handles itself (a Resizable handle: arrows, Home, End, Enter). */
fun SurfaceModel.nodeKey(index: Int, key: String) {
    fire(index, "key", JsonValue.Obj(mapOf("key" to JsonValue.Str(key))))
}

/** VAPP-162: a `hold: true` Button (round-5 contract §9; the core owns the hold, its `pressed` state and `holdingLabel`). */
fun NodeInfo.isHoldButton(): Boolean = component == "Button" && part == null && props["hold"]?.bool == true

/**
 * VAPP-162: one input on a `hold` Button through the core's `Surface.hold`:
 * `{do: pointerdown | pointermove | pointerup, x, y}` (dp from the button's
 * top-left), `{do: pointercancel}`, `{do: keydown, key, repeat?}` (`" "`,
 * `Enter`, `Escape`), `{do: keyup, key}`, `{do: blur}`. Dispatches the
 * holdStart / holdEnd / holdCancel actions and relays out.
 */
fun SurfaceModel.hold(index: Int, input: JsonValue) {
    val events = try {
        surface.hold(index.toUInt(), input.orderedJson)
    } catch (_: UiException) {
        return
    }
    if (events.isNotEmpty()) dispatch(events)
}

/** [hold] with a pointer at ([x], [y]) dp from the button's top-left. */
fun SurfaceModel.holdPointer(index: Int, phase: String, x: Float, y: Float) =
    hold(index, JsonValue.Obj(linkedMapOf("do" to JsonValue.Str(phase), "x" to JsonValue.Num(x.toDouble()), "y" to JsonValue.Num(y.toDouble()))))

/** [hold] with an input that carries only `do` (`pointercancel`, `blur`) or a key. */
fun SurfaceModel.holdInput(index: Int, phase: String, key: String? = null, repeat: Boolean = false) =
    hold(
        index,
        JsonValue.Obj(
            linkedMapOf<String, JsonValue>("do" to JsonValue.Str(phase)).apply {
                if (key != null) put("key", JsonValue.Str(key))
                if (repeat) put("repeat", JsonValue.Bool(true))
            },
        ),
    )

/** Is the `hold` Button [id] held now (the core's state)? */
fun SurfaceModel.isHeld(id: String): Boolean = runCatching { surface.isHeld(id) }.getOrDefault(false)

/** Fire `event` on node `index` through the core and dispatch the result. */
fun SurfaceModel.fire(index: Int, event: String, payload: JsonValue? = null) {
    val events = try {
        surface.event(index.toUInt(), event, payload?.json)
    } catch (_: UiException) {
        return
    }
    dispatch(copyPartText(index, events))
}

/**
 * EXP-1324: a CodeBlock `copy` part carries `text` = what to copy (ANSI
 * escapes stripped); the core's Copy event carries the owner's raw `code`,
 * so the part's `text` wins when present.
 */
internal fun SurfaceModel.copyPartText(index: Int, events: List<FfiEvent>): List<FfiEvent> {
    val n = node(index) ?: return events
    if (n.ownerComponent != "CodeBlock" || n.part != "copy") return events
    val text = n.props["text"]?.string ?: return events
    return events.map { e ->
        if (e.kind != "copy") e
        else e.copy(json = JsonValue.Obj(mapOf("text" to JsonValue.Str(text))).json)
    }
}

/** A mirrored control value: the local one while the prop is unchanged. */
internal fun SurfaceModel.mirrored(ownerId: String, external: JsonValue): JsonValue {
    val m = mirrors[ownerId]
    return if (m != null && m.external == external) m.local else external
}

internal fun SurfaceModel.setMirror(ownerId: String, external: JsonValue, local: JsonValue) {
    mirrors[ownerId] = Mirror(external, local)
    mirrorGeneration += 1
}

private fun lastSegment(id: String): String = id.split('.').lastOrNull() ?: ""

/** A full press of node `index` (a tap, Enter / Space). */
fun SurfaceModel.press(index: Int) {
    if (isDisabled(index)) return
    val n = node(index) ?: return
    val owner = owner(index) ?: n
    // A Select / DatePicker trigger under native overlays opens the platform
    // picker (`popup`); otherwise (painted overlays, a painted Select, the
    // other pickers) the core opens the owner's popup layer.
    if (n.isPickerTrigger && nativePicker(n)) {
        popup = if (popup == n.id) null else n.id
        // #EXP-1629: the platform menu opens outside the core's layers; a
        // Select's `open` still fires through it.
        if (popup == n.id && (owner.component == "Select" || n.ownerComponent == "Select")) fire(index, "open")
        return
    }
    val target = n.triggerFor
    if (target != null) {
        if (justDismissed == target) {
            justDismissed = null
            return
        }
        layerReturn[target] = n.id
        fire(index, "press")
        return
    }
    if (n.isTextField || (n.component == "Slider" && n.part == "track") || n.component == "Segmented") return
    // #VAPP-288: the focused field's outstanding edit goes out first.
    flushFocusedField()
    // EXP-1605: a Motion's state machine gets the click first.
    motionDrivers[n.id]?.post("click")
    when (owner.component) {
        "Checkbox", "Switch" -> {
            val external = owner.props["checked"] ?: JsonValue.Bool(false)
            val current = mirrored(owner.id, external).bool ?: false
            // Round 8 (interactions-round1.json `checkbox`): a mixed box
            // resolves to checked (the core writes `indeterminate: false`).
            val next = if (mixed(owner.index)) true else !current
            setMirror(owner.id, external, JsonValue.Bool(next))
            val events = try {
                surface.event(owner.index.toUInt(), "change", JsonValue.Obj(mapOf("checked" to JsonValue.Bool(next))).json)
            } catch (_: UiException) {
                null
            }
            if (events != null) dispatch(events)
            return
        }
        "Radio" -> {
            val row = indexOf("${owner.id}.item.${lastSegment(n.id)}")?.let(::node)
            if (row != null) {
                row.props["value"]?.let { setMirror(owner.id, owner.props["value"] ?: JsonValue.Null, it) }
                fire(row.index, "press")
                return
            }
        }
        "Toggle" -> {
            val external = owner.props["pressed"] ?: JsonValue.Bool(false)
            val current = mirrored(owner.id, external).bool ?: false
            setMirror(owner.id, external, JsonValue.Bool(!current))
        }
    }
    fire(index, "press")
}

/** Press the node with this id. */
fun SurfaceModel.press(id: String) {
    indexOf(id)?.let { press(it) }
}

// states

internal fun SurfaceModel.setInteraction(id: String, change: (InteractionState) -> InteractionState): Boolean {
    val before = interaction[id] ?: InteractionState()
    val s = change(before)
    if (s == before) return false
    interaction[id] = s
    if (surface.setStates(id, s.states)) {
        layoutNeeded = true
        pass()
    }
    return true
}

/** Pointer down on a pressable: the `pressed` state. */
fun SurfaceModel.pressDown(id: String) {
    val prev = pressedId
    if (prev != null && prev != id) setInteraction(prev) { it.copy(pressed = false) }
    val i = indexOf(id)
    if (i != null && isDisabled(i)) return
    pressedId = id
    motionDrivers[id]?.post("pointerDown")
    setInteraction(id) { it.copy(pressed = true) }
}

/** Pointer up: clear the state; the press itself is the click's action. */
fun SurfaceModel.pressUp(id: String) {
    if (pressedId == id) pressedId = null
    motionDrivers[id]?.post("pointerUp")
    setInteraction(id) { it.copy(pressed = false) }
}

/** Pointer hover on / off a node. */
fun SurfaceModel.hover(id: String, hovered: Boolean) {
    // #EXP-1629: over a Select option the pointer moves the popup's cursor (its one `hover`); leaving keeps it.
    val n = indexOf(id)?.let { node(it) }
    if (n != null && n.ownerComponent == "Select" && n.part == "item") {
        if (hovered && !isDisabled(n.index)) n.owner?.let { setSelectCursor(it, id) }
        return
    }
    // #EXP-1629: a row Select's value button lends its `hover` to the value/placeholder/chevron inside it; theirs alone is never reported.
    if (n != null && n.ownerComponent == "Select") {
        if (n.part in setOf("value", "placeholder", "chevron") && n.parent?.let { node(it) }?.part == "button") return
        if (n.part == "button") for (c in n.children) node(c)?.let { child -> setInteraction(child.id) { it.copy(hover = hovered) } }
    }
    setInteraction(id) { it.copy(hover = hovered) }
}

/** #EXP-1629: move Select [owner]'s popup cursor to item [id] (null = none): the old item loses `hover`, the new one gets it. */
fun SurfaceModel.setSelectCursor(owner: String, id: String?) {
    if (id != null) selectCursorless.remove(owner)
    val prev = selectCursors[owner]
    if (prev == id) return
    if (id == null) selectCursors.remove(owner) else selectCursors[owner] = id
    prev?.let { p -> setInteraction(p) { it.copy(hover = false) } }
    id?.let { c -> setInteraction(c) { it.copy(hover = true) } }
}

/**
 * #EXP-1629: an open Select popup (the core's layer, its list at the top)
 * puts its cursor on the FIRST enabled option (a fresh pick, the IDE's
 * picker; a `multiple` one's on its first chosen, the IDE's multi picker) and focuses its search field (else, opened from the keyboard,
 * that option); a cursor whose option went (a search) starts over. A
 * closed popup drops its cursor.
 */
private fun SurfaceModel.syncSelectCursors() {
    val open = layers.filter { it.kind == "Select" }.map { it.owner }.toSet()
    for (o in selectCursors.keys - open) setSelectCursor(o, null)
    selectCursorless.retainAll(open)
    for (o in open) {
        val cur = selectCursors[o]
        if (cur != null && indexOf(cur) != null) continue
        if (o in selectCursorless) continue
        val opening = cur == null
        // #EXP-1765: a `pill` opened by the POINTER shows no cursor until
        // the pointer moves over an option or a key goes ([selectWakeCursor]).
        if (opening && !keyboardDriven && indexOf(o)?.let { node(it) }?.props?.str("triggerVariant") == "pill") {
            selectCursorless += o
            nodes.firstOrNull { it.owner == o && it.ownerComponent == "Select" && it.part == "search" && !it.hidden }?.let { focusRequest = it.id }
            continue
        }
        val items = nodes.filter { it.owner == o && it.ownerComponent == "Select" && it.part == "item" && !it.hidden }
        val multiple = opening && indexOf(o)?.let { node(it) }?.props?.flag("multiple") == true
        val target = (if (multiple) items.firstOrNull { !isDisabled(it.index) && it.selected } else items.firstOrNull { !isDisabled(it.index) }) ?: continue
        setSelectCursor(o, target.id)
        if (!opening) continue
        val search = nodes.firstOrNull { it.owner == o && it.ownerComponent == "Select" && it.part == "search" && !it.hidden }
        if (search != null) focusRequest = search.id
        else if (keyboardDriven) focusRequest = target.id
    }
}

/** Is the keyboard driving (the focused node shows its focus ring: a Tab / arrow focus, not a tap)? */
internal val SurfaceModel.keyboardDriven: Boolean get() = keyboardFocus?.let { interaction[it]?.focusVisible } == true

/**
 * #EXP-1765: a key went while [n] (a Select's trigger, search or option)
 * has focus: a cursorless pill popup gets its cursor (the first enabled
 * option; a `multiple` one's first chosen). True when it woke one.
 */
fun SurfaceModel.selectWakeCursor(n: NodeInfo): Boolean {
    // The trigger names its Select through `triggerFor`; the search and the options through their owner.
    val o = n.triggerFor ?: n.owner ?: return false
    if (!selectCursorless.remove(o)) return false
    val items = nodes.filter { it.owner == o && it.ownerComponent == "Select" && it.part == "item" && !it.hidden }
    val multiple = indexOf(o)?.let { node(it) }?.props?.flag("multiple") == true
    val target = (if (multiple) items.firstOrNull { !isDisabled(it.index) && it.selected } else null) ?: items.firstOrNull { !isDisabled(it.index) } ?: return true
    setSelectCursor(o, target.id)
    // Without a search field the keys go on from the option.
    if (nodes.none { it.owner == o && it.ownerComponent == "Select" && it.part == "search" && !it.hidden }) focusRequest = target.id
    return true
}

/**
 * #EDE-65: the pointer entered a Box: its `on.hover` fires, exactly like a
 * Motion's (no payload); leaving fires nothing. Pressable or not; nothing
 * without a handler.
 */
fun SurfaceModel.boxHover(id: String, inside: Boolean) {
    if (!inside) return
    val i = indexOf(id) ?: return
    val n = node(i) ?: return
    if (n.component != "Box" || n.part != null) return
    val events = try {
        surface.event(i.toUInt(), "hover", null)
    } catch (_: UiException) {
        return
    }
    if (events.isNotEmpty()) dispatch(events)
}

// #EXP-1629 (interactions-round1.json keyRecorder): a KeyRecorder's field.
// The painter captures (keys, mouse buttons 3-5, focus loss) and sends
// `press`, `record` {type, key?, button?, ctrl, alt, shift, meta} and
// `blur` on the field; the core guards them (a press only while idle,
// record / blur only while recording) and the host owns the grammar.

/**
 * A KeyRecorder's `field` part: the macro's one pressable Box (a macro's
 * nodes carry the macro's name, never their part; the part's id is
 * `<owner>.field`).
 */
val NodeInfo.isKeyRecorderField: Boolean get() = macroName == "KeyRecorder" && component == "Box" && pressable && id.endsWith(".field")

/**
 * The field records now: its `recording` recipe prop as the core resolved
 * it (the field's a11y `pressed` = `{props.recording|false}`), read live.
 */
val NodeInfo.keyRecorderRecording: Boolean get() = isKeyRecorderField && accessibility?.get("pressed")?.bool == true

/** Is the KeyRecorder field at [index] recording (the live node)? */
fun SurfaceModel.keyRecorderRecording(index: Int): Boolean = node(index)?.keyRecorderRecording == true

/** The `record` payload: the event [type], its canonical [key] or mouse [button], and the held flags. */
fun keyRecorderPayload(type: String, ctrl: Boolean, alt: Boolean, shift: Boolean, meta: Boolean, key: String? = null, button: Int? = null): JsonValue =
    JsonValue.Obj(
        linkedMapOf<String, JsonValue>(
            "ctrl" to JsonValue.Bool(ctrl),
            "alt" to JsonValue.Bool(alt),
            "shift" to JsonValue.Bool(shift),
            "meta" to JsonValue.Bool(meta),
            "type" to JsonValue.Str(type),
        ).apply {
            if (key != null) put("key", JsonValue.Str(key))
            if (button != null) put("button", JsonValue.Num(button.toDouble()))
        },
    )

/** A press on the field (a tap, or Enter / Space while idle): `start` + recording true while idle. */
fun SurfaceModel.keyRecorderPress(index: Int) {
    if (node(index)?.isKeyRecorderField != true) return
    fire(index, "press")
}

/** A captured key / modifier change / mouse button ([keyRecorderPayload]) while recording: `key`. */
fun SurfaceModel.keyRecorderRecord(index: Int, payload: JsonValue) {
    if (node(index)?.isKeyRecorderField != true) return
    fire(index, "record", payload)
}

/** A mouse button over the field: only 3 (middle), 4 (back) and 5 (forward) are sent. */
fun SurfaceModel.keyRecorderMouse(index: Int, button: Int, ctrl: Boolean = false, alt: Boolean = false, shift: Boolean = false, meta: Boolean = false) {
    if (button !in 3..5) return
    keyRecorderRecord(index, keyRecorderPayload("mouse", ctrl, alt, shift, meta, button = button))
}

/** The field lost the focus: `stop` + recording false while recording. */
fun SurfaceModel.keyRecorderBlur(index: Int) {
    if (node(index)?.isKeyRecorderField != true) return
    fire(index, "blur")
}

/** Focus on / off a node. */
fun SurfaceModel.focus(id: String, focused: Boolean, fromKeyboard: Boolean = false) {
    if (focused) keyboardFocus = id else if (keyboardFocus == id) keyboardFocus = null
    setInteraction(id) { it.copy(focus = focused, focusVisible = focused && fromKeyboard) }
}

// overlays

/** Open or close an overlay owner (Dialog, Drawer, Popover, Tooltip, Menu). */
fun SurfaceModel.setOpen(owner: String, open: Boolean) {
    dispatch(surface.setOpen(owner, open))
}

/**
 * Is `layer` a Menu's open SUBMENU (round 3: the core lays it out as a
 * second layer of the same owner, root `<owner>.content.<row>`, beside its
 * row), not the menu itself?
 */
fun SurfaceModel.isSubmenuLayer(layer: LayerInfo): Boolean =
    layer.kind == "Menu" && node(layer.root)?.id != "${layer.owner}.content"

/**
 * #EXP-1629: is `layer` a Menu item's `preview` (the core's `preview` part
 * beside its row while hovered or highlighted)? It never takes the focus or
 * Escape; the menu under it keeps them (gpui `passive_layer`).
 */
fun SurfaceModel.isMenuPreviewLayer(layer: LayerInfo): Boolean =
    layer.kind == "Menu" && node(layer.root)?.let { it.ownerComponent == "Menu" && it.part == "preview" } == true

/**
 * #EDE-65 (interactions-round1.json typeaheadFocus): is `layer` a TYPEAHEAD,
 * a Popover whose `anchor` (the node it linked as its trigger) is a text
 * field (a Composer, an Input, a Textarea: the node itself or its `field`
 * part)? Opening it never moves the focus (the field keeps it, its caret and
 * the IME) and closing it returns the focus nowhere; it keeps Escape (gpui
 * `Surface::is_typeahead`).
 */
fun SurfaceModel.isTypeaheadLayer(layer: LayerInfo): Boolean {
    if (layer.kind != "Popover") return false
    val owner = indexOf(layer.owner)?.let { node(it) } ?: return false
    if (owner.component != "Popover" || owner.props.str("anchor").isEmpty()) return false
    val anchor = nodes.firstOrNull { !it.removed && it.triggerFor == layer.owner } ?: return false
    return isFieldAnchor(if (anchor.part == null) anchor.component else anchor.ownerComponent ?: anchor.component, anchor.part)
}

/** `is_field_anchor` (Rust) / `isFieldAnchor` (TS). */
fun isFieldAnchor(component: String, part: String?): Boolean =
    component in setOf("Composer", "Input", "Textarea") && (part == null || part == "root" || part == "field")

/**
 * Does the popup of `layer` take the window focus when it opens? Never a Menu
 * item's preview, never a typeahead ([isTypeaheadLayer]).
 */
fun SurfaceModel.popupTakesFocus(layer: LayerInfo): Boolean = !isMenuPreviewLayer(layer) && !isTypeaheadLayer(layer)

/** Close the open submenu of the Menu `owner` (its row toggles it), the menu stays open. */
fun SurfaceModel.closeSubmenu(owner: String) {
    val row = nodes.firstOrNull { it.owner == owner && it.ownerComponent == "Menu" && it.part == "item" && it.open && it.props.str("kind") == "submenu" } ?: return
    fire(row.index, "press")
}

/**
 * Close the overlay `owner` (a swipe, the scrim, back): the core's
 * `dismiss` on the layer root (it honours `dismissible` and fires the
 * author's `dismiss`); a toast through `dismissToast`.
 */
fun SurfaceModel.dismissLayer(owner: String) {
    val layer = layers.firstOrNull { it.owner == owner } ?: return
    justDismissed = owner
    val events = try {
        if (layer.isToast) surface.dismissToast(owner) else surface.event(layer.root.toUInt(), "dismiss", null)
    } catch (_: UiException) {
        surface.setOpen(owner, false)
    }
    dispatch(events)
    // The remembered dismissal only guards the trigger's own tap.
    scope.launch {
        delay(300)
        if (justDismissed == owner) justDismissed = null
    }
}

/**
 * Escape / back (the gpui painter's rule): close the native picker popup,
 * else the TOP overlay (a pinned one presses its `.cancel`, an
 * AlertDialog), else a tooltip. Toasts are never escaped. Returns whether
 * something handled it.
 */
fun SurfaceModel.escape(): Boolean {
    if (popup != null) {
        popup = null
        return true
    }
    val top = layers.lastOrNull { !it.isToast && it.kind != "Tooltip" && !isMenuPreviewLayer(it) }
    if (top != null) {
        if (top.dismissible) {
            justDismissed = null
            fire(top.root, "dismiss")
        } else {
            val ids = nodes.filter { it.layer == top.layer && !it.hidden }
            ids.firstOrNull { it.id.endsWith(".cancel") && it.pressable }?.let { press(it.index) }
        }
        return true
    }
    val tip = layers.lastOrNull { it.kind == "Tooltip" } ?: return false
    dispatch(surface.setOpen(tip.owner, false))
    return true
}

/** Hover on a Tooltip anchor (pointer devices): open after 300 ms, close on leave. */
fun SurfaceModel.tooltipHover(owner: String, hovered: Boolean) {
    tooltipJob?.cancel()
    if (hovered) {
        tooltipJob = scope.launch {
            delay(300)
            if (isActive) dispatch(surface.setOpen(owner, true))
        }
    } else if (layers.any { it.owner == owner }) {
        dispatch(surface.setOpen(owner, false))
    }
}

/** The modal layers (Dialog / Drawer) open, in order. */
val SurfaceModel.modalLayers: List<LayerInfo> get() = layers.filter { it.isModal }

internal fun SurfaceModel.layersChanged() {
    val now = layers.map { it.owner }.toSet()
    layerReturn.keys.retainAll(now)
    syncSelectCursors()
}

// controls

/** A Segmented item press: single = that value, multiple = the toggled set. */
fun SurfaceModel.segmentedSelect(index: Int, value: JsonValue) {
    val n = node(index) ?: return
    if (isDisabled(index)) return
    val multiple = n.props.str("type") == "multiple"
    val external = n.props["value"] ?: JsonValue.Null
    val current = mirrored(n.id, external)
    val next: JsonValue = if (multiple) {
        val set: MutableList<JsonValue> = when {
            current is JsonValue.Arr -> current.v.toMutableList()
            current is JsonValue.Str && current.v.isNotEmpty() -> current.v.split(',').map { JsonValue.Str(it) }.toMutableList()
            else -> mutableListOf()
        }
        val p = set.indexOf(value)
        if (p >= 0) set.removeAt(p) else set.add(value)
        JsonValue.Arr(set)
    } else {
        value
    }
    setMirror(n.id, external, next)
    fire(index, "change", JsonValue.Obj(mapOf("value" to next)))
}

/** The values a Segmented shows selected. */
fun SurfaceModel.segmentedValues(index: Int): List<String> {
    val n = node(index) ?: return emptyList()
    return when (val v = mirrored(n.id, n.props["value"] ?: JsonValue.Null)) {
        is JsonValue.Arr -> v.v.map { it.displayText }
        is JsonValue.Str -> if (v.v.isEmpty()) emptyList() else v.v.split(',')
        JsonValue.Null -> emptyList()
        else -> listOf(v.displayText)
    }
}

/** The checked state of a Checkbox / Switch (mirror over prop). */
fun SurfaceModel.checked(index: Int): Boolean {
    val o = owner(index) ?: return false
    return mirrored(o.id, o.props["checked"] ?: JsonValue.Bool(false)).bool ?: false
}

/**
 * Round 8: a MIXED Checkbox (`indeterminate`, bound or the core's local
 * `<id>.indeterminate` after a press resolved it): it wins over [checked]
 * (a11y.json: ToggleableState.Indeterminate; the dash glyph).
 */
fun SurfaceModel.mixed(index: Int): Boolean {
    val o = owner(index) ?: return false
    return o.component == "Checkbox" && o.props["indeterminate"]?.bool == true
}

/** Is this Radio dot / row the chosen one? */
fun SurfaceModel.radioChecked(index: Int): Boolean {
    val n = node(index) ?: return false
    val o = owner(index) ?: return false
    val value = mirrored(o.id, o.props["value"] ?: JsonValue.Null)
    val row = indexOf("${o.id}.item.${lastSegment(n.id)}")?.let(::node) ?: return false
    val rv = row.props["value"] ?: return false
    return rv.displayText == value.displayText
}

/** A Toggle's pressed state (mirror over prop). */
fun SurfaceModel.togglePressed(index: Int): Boolean {
    val n = node(index) ?: return false
    return mirrored(n.id, n.props["pressed"] ?: JsonValue.Bool(false)).bool ?: false
}

/** A Select's value (mirror over prop). */
fun SurfaceModel.selectValue(index: Int): JsonValue {
    val o = owner(index) ?: return JsonValue.Null
    return mirrored(o.id, o.props["value"] ?: JsonValue.Null)
}

/** A Select pick: single = the value, multiple = the toggled set. */
fun SurfaceModel.selectPick(fieldIndex: Int, value: JsonValue) {
    val o = owner(fieldIndex) ?: return
    val external = o.props["value"] ?: JsonValue.Null
    val next: JsonValue = if (o.props.flag("multiple")) {
        val cur = mirrored(o.id, external)
        val set = (cur.array ?: if (cur.isNull) emptyList() else listOf(cur)).toMutableList()
        val p = set.indexOf(value)
        if (p >= 0) set.removeAt(p) else set.add(value)
        JsonValue.Arr(set)
    } else {
        // VAPP-146: "" is an ordinary value; the option that holds the
        // current one ("" when unseeded) fires nothing.
        val cur = mirrored(o.id, external)
        if ((if (cur.isNull) "" else cur.displayText) == value.displayText) return
        value
    }
    setMirror(o.id, external, next)
    fire(fieldIndex, "change", JsonValue.Obj(mapOf("value" to next)))
}

/** A DatePicker's ISO value (mirror over prop). */
fun SurfaceModel.dateValue(index: Int): String {
    val o = owner(index) ?: return ""
    return mirrored(o.id, o.props["value"] ?: JsonValue.Null).displayText
}

/** A DatePicker day pick (closes the popup). */
fun SurfaceModel.pickDate(fieldIndex: Int, iso: String) {
    val o = owner(fieldIndex) ?: return
    setMirror(o.id, o.props["value"] ?: JsonValue.Null, JsonValue.Str(iso))
    popup = null
    fire(fieldIndex, "change", JsonValue.Obj(mapOf("value" to JsonValue.Str(iso))))
}

/** The value a slider track shows (drag > mirror > prop > min). */
fun SurfaceModel.sliderValue(index: Int): Double {
    val n = node(index) ?: return 0.0
    drags[n.id]?.let { return it }
    val o = owner(index) ?: n
    return mirrored(o.id, n.props["value"] ?: JsonValue.Null).number ?: n.props.num("min") ?: 0.0
}

/** A slider drag in flight (the caller snaps `value`). */
fun SurfaceModel.sliderDrag(index: Int, value: Double) {
    val n = node(index) ?: return
    if (isDisabled(index)) return
    if (drags[n.id] != value) {
        drags[n.id] = value
        mirrorGeneration += 1
    }
}

/** The drag ended: mirror the value and fire `change`. */
fun SurfaceModel.sliderRelease(index: Int) {
    val n = node(index) ?: return
    val value = drags.remove(n.id) ?: return
    val o = owner(index) ?: n
    setMirror(o.id, n.props["value"] ?: JsonValue.Null, JsonValue.Num(value))
    fire(index, "change", JsonValue.Obj(mapOf("value" to JsonValue.Num(value))))
}

/** A Carousel page change (`index` = the carousel or its indicator). */
fun SurfaceModel.carouselPage(index: Int, page: Int) {
    val o = owner(index) ?: return
    fire(o.index, "change", JsonValue.Obj(mapOf("page" to JsonValue.Num(page.toDouble()))))
}

/** Scroll a windowed list to a content offset (dp; relayouts only when the window moved). */
fun SurfaceModel.scroll(listId: String, offset: Float) {
    if (surface.scroll(listId, offset)) {
        // The window's rows change without a structure-version bump.
        nodesDirty = true
        layoutNeeded = true
        pass()
    }
}

