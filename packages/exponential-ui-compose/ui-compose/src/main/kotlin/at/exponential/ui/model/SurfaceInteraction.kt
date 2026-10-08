package at.exponential.ui.model

import at.exponential.ui.ffi.FfiEvent
import at.exponential.ui.ffi.UiException
import at.exponential.ui.host.InputKind
import at.exponential.ui.host.SurfaceActionEvent
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
            else -> {}
        }
    }
    nodesDirty = true
    layoutNeeded = true
    pass()
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
    when {
        n.component == "DatePicker" && n.part == "field" -> {
            popup = if (popup == n.id) null else n.id
            return
        }
        (n.component == "Select" && n.part == "field") || (n.component == "Input" && n.part == "field") ||
            (n.component == "Textarea" && n.part == "field") || (n.component == "Slider" && n.part == "track") ||
            (n.component == "Composer" && n.part == null) || n.component == "ToggleGroup" -> return
    }
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
fun SurfaceModel.focus(id: String, focused: Boolean) {
    setInteraction(id) { it.copy(focus = focused) }
}

// overlays

/** Open or close an overlay owner (Dialog, Drawer, Popover, Tooltip, DropdownMenu). */
fun SurfaceModel.setOpen(owner: String, open: Boolean) {
    dispatch(surface.setOpen(owner, open))
}

/** Close the overlay `owner` (a swipe, the scrim, back). */
fun SurfaceModel.dismissLayer(owner: String) {
    if (layers.none { it.owner == owner }) return
    justDismissed = owner
    dispatch(surface.setOpen(owner, false))
    // The remembered dismissal only guards the trigger's own tap.
    scope.launch {
        delay(300)
        if (justDismissed == owner) justDismissed = null
    }
}

/** Escape / back: close the date popup, else the TOP layer. Returns whether something closed. */
fun SurfaceModel.escape(): Boolean {
    if (popup != null) {
        popup = null
        return true
    }
    val top = layers.lastOrNull() ?: return false
    dispatch(surface.setOpen(top.owner, false))
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

