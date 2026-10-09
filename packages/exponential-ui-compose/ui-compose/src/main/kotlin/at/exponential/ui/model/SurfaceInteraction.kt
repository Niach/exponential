package at.exponential.ui.model

import at.exponential.ui.ffi.FfiEvent
import at.exponential.ui.ffi.UiException
import at.exponential.ui.host.FilePickRequest
import at.exponential.ui.host.InputKind
import at.exponential.ui.host.PickedFile
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.host.SurfaceFunctionCallEvent
import at.exponential.ui.host.SurfaceInputEvent
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
    for (e in events) {
        val v = JsonValue.parse(e.json)
        when (e.kind) {
            "action" -> host.onAction(
                SurfaceActionEvent(
                    surfaceId = id,
                    event = v["event"]?.string ?: "",
                    name = v["name"]?.string ?: "",
                    componentId = v["component_id"]?.string ?: "",
                    context = v["context"] ?: JsonValue.Obj(emptyMap()),
                    payload = v["payload"],
                ),
            )
            "openUrl" -> host.openUrl(v["url"]?.string ?: "")
            "functionCall" -> host.onFunctionCall(
                SurfaceFunctionCallEvent(
                    surfaceId = id,
                    componentId = v["componentId"]?.string ?: v["component_id"]?.string ?: "",
                    name = v["name"]?.string ?: "",
                    args = v["args"]?.obj ?: emptyMap(),
                ),
            )
            "input" -> {
                val component = v["component_id"]?.string ?: ""
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
                FilePickRequest(this, v["component_id"]?.string ?: "", v["accept"]?.string, v["multiple"]?.bool ?: false),
            )
            "hoverTimer" -> {
                val owner = v["owner"]?.string ?: ""
                val delayMs = (v["delay_ms"]?.number ?: 0.0).toLong()
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

/** A key on a node the core handles itself (a Resizable handle: arrows, Home, End, Enter). */
fun SurfaceModel.nodeKey(index: Int, key: String) {
    fire(index, "key", JsonValue.Obj(mapOf("key" to JsonValue.Str(key))))
}

/** Fire `event` on node `index` through the core and dispatch the result. */
fun SurfaceModel.fire(index: Int, event: String, payload: JsonValue? = null) {
    val events = try {
        surface.event(index.toUInt(), event, payload?.json)
    } catch (_: UiException) {
        return
    }
    dispatch(events)
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
    if (n.isTextField || (n.component == "Slider" && n.part == "track") || n.component == "ToggleGroup") return
    when (owner.component) {
        "Checkbox", "Switch" -> {
            val external = owner.props["checked"] ?: JsonValue.Bool(false)
            val current = mirrored(owner.id, external).bool ?: false
            setMirror(owner.id, external, JsonValue.Bool(!current))
            val events = try {
                surface.event(owner.index.toUInt(), "change", JsonValue.Obj(mapOf("checked" to JsonValue.Bool(!current))).json)
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
    setInteraction(id) { it.copy(pressed = true) }
}

/** Pointer up: clear the state; the press itself is the click's action. */
fun SurfaceModel.pressUp(id: String) {
    if (pressedId == id) pressedId = null
    setInteraction(id) { it.copy(pressed = false) }
}

/** Pointer hover on / off a node. */
fun SurfaceModel.hover(id: String, hovered: Boolean) {
    setInteraction(id) { it.copy(hover = hovered) }
}

/** Focus on / off a node. */
fun SurfaceModel.focus(id: String, focused: Boolean, fromKeyboard: Boolean = false) {
    if (focused) keyboardFocus = id else if (keyboardFocus == id) keyboardFocus = null
    setInteraction(id) { it.copy(focus = focused, focusVisible = focused && fromKeyboard) }
}

// overlays

/** Open or close an overlay owner (Dialog, Drawer, Popover, Tooltip, DropdownMenu). */
fun SurfaceModel.setOpen(owner: String, open: Boolean) {
    dispatch(surface.setOpen(owner, open))
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
    val top = layers.lastOrNull { !it.isToast && it.kind != "Tooltip" }
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
}

// controls

/** A ToggleGroup item press: single = that value, multiple = the toggled set. */
fun SurfaceModel.toggleGroupSelect(index: Int, value: JsonValue) {
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

/** The values a ToggleGroup shows selected. */
fun SurfaceModel.toggleGroupValues(index: Int): List<String> {
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

