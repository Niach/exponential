package at.exponential.ui.compose

import android.view.InputDevice
import android.view.KeyCharacterMap
import android.view.KeyEvent as AndroidKeyEvent
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.key.utf16CodePoint
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.carouselPage
import at.exponential.ui.model.escape
import at.exponential.ui.model.fire
import at.exponential.ui.model.holdInput
import at.exponential.ui.model.isHeld
import at.exponential.ui.model.isHoldButton
import at.exponential.ui.model.isKeyRecorderField
import at.exponential.ui.model.keyRecorderPayload
import at.exponential.ui.model.keyRecorderRecord
import at.exponential.ui.model.keyRecorderRecording
import at.exponential.ui.model.nodeKey
import at.exponential.ui.model.press
import at.exponential.ui.model.scrollTo
import at.exponential.ui.model.sliderValue
import at.exponential.ui.model.segmentedSelect
import at.exponential.ui.model.setSelectCursor
import at.exponential.ui.model.segmentedValues
import at.exponential.ui.model.searchKey

/** A hardware key as the a11y spec names it (`left`, `enter`, `pageup`…), null for the rest. */
internal fun keyName(e: KeyEvent): String? = when (e.key) {
    Key.DirectionLeft -> "left"
    Key.DirectionRight -> "right"
    Key.DirectionUp -> "up"
    Key.DirectionDown -> "down"
    Key.MoveHome -> "home"
    Key.MoveEnd -> "end"
    Key.PageUp -> "pageup"
    Key.PageDown -> "pagedown"
    Key.Enter, Key.NumPadEnter -> "enter"
    Key.Spacebar -> "space"
    Key.Escape, Key.Back -> "escape"
    Key.F10 -> "f10"
    Key.Backspace -> "backspace"
    else -> null
}

/**
 * The surface's hardware keyboard (`catalog/a11y.json` keys, the gpui
 * painter's rules): Escape closes the top layer; Enter / Space press the
 * focused control; arrows rove inside tabs, radios, toggle groups, menus
 * and listboxes (Home / End = the ends) and step sliders, carousels and
 * Resizable handles (the core's `keyboardResize`); ArrowDown opens a
 * picker or menu trigger; Shift+F10 opens a context Menu; a focused scroll
 * container scrolls. Tab moves focus in pre-order (Compose's focus order
 * follows composition = pre-order). Text fields keep their keys.
 */
internal fun Modifier.surfaceKeys(model: SurfaceModel): Modifier = onPreviewKeyEvent { e ->
    keyRecorderKey(model, e)?.let { return@onPreviewKeyEvent it }
    holdKey(model, e)?.let { return@onPreviewKeyEvent it }
    if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
    val key = keyName(e)
    val typed = e.utf16CodePoint.takeIf { it > 32 }?.let { String(Character.toChars(it)) }
    handleKey(model, key, e.isShiftPressed, typed)
}

/**
 * #EXP-1629: the CANONICAL KeyRecorder name of an Android key code (the
 * physical key, never the shifted character; the same names on every
 * painter): a-z, 0-9, f1-f24, the named keys and the US-layout punctuation.
 * Null = not sent (numpad digits, media keys, the modifiers themselves).
 */
fun keyRecorderKeyName(keyCode: Int): String? = when (keyCode) {
    in AndroidKeyEvent.KEYCODE_A..AndroidKeyEvent.KEYCODE_Z -> ('a' + (keyCode - AndroidKeyEvent.KEYCODE_A)).toString()
    in AndroidKeyEvent.KEYCODE_0..AndroidKeyEvent.KEYCODE_9 -> ('0' + (keyCode - AndroidKeyEvent.KEYCODE_0)).toString()
    in AndroidKeyEvent.KEYCODE_F1..AndroidKeyEvent.KEYCODE_F12 -> "f${keyCode - AndroidKeyEvent.KEYCODE_F1 + 1}"
    // F13-F24 (API 35 codes 326-337; compile-time constants, so any device
    // that reports them is read).
    in AndroidKeyEvent.KEYCODE_F13..AndroidKeyEvent.KEYCODE_F24 -> "f${keyCode - AndroidKeyEvent.KEYCODE_F13 + 13}"
    AndroidKeyEvent.KEYCODE_SPACE -> "space"
    AndroidKeyEvent.KEYCODE_ENTER, AndroidKeyEvent.KEYCODE_NUMPAD_ENTER -> "enter"
    AndroidKeyEvent.KEYCODE_TAB -> "tab"
    AndroidKeyEvent.KEYCODE_ESCAPE, AndroidKeyEvent.KEYCODE_BACK -> "escape"
    AndroidKeyEvent.KEYCODE_DEL -> "backspace"
    AndroidKeyEvent.KEYCODE_FORWARD_DEL -> "delete"
    AndroidKeyEvent.KEYCODE_INSERT -> "insert"
    AndroidKeyEvent.KEYCODE_MOVE_HOME -> "home"
    AndroidKeyEvent.KEYCODE_MOVE_END -> "end"
    AndroidKeyEvent.KEYCODE_PAGE_UP -> "pageup"
    AndroidKeyEvent.KEYCODE_PAGE_DOWN -> "pagedown"
    AndroidKeyEvent.KEYCODE_DPAD_UP -> "up"
    AndroidKeyEvent.KEYCODE_DPAD_DOWN -> "down"
    AndroidKeyEvent.KEYCODE_DPAD_LEFT -> "left"
    AndroidKeyEvent.KEYCODE_DPAD_RIGHT -> "right"
    AndroidKeyEvent.KEYCODE_MINUS -> "-"
    AndroidKeyEvent.KEYCODE_EQUALS -> "="
    AndroidKeyEvent.KEYCODE_LEFT_BRACKET -> "["
    AndroidKeyEvent.KEYCODE_RIGHT_BRACKET -> "]"
    AndroidKeyEvent.KEYCODE_BACKSLASH -> "\\"
    AndroidKeyEvent.KEYCODE_SEMICOLON -> ";"
    AndroidKeyEvent.KEYCODE_APOSTROPHE -> "'"
    AndroidKeyEvent.KEYCODE_COMMA -> ","
    AndroidKeyEvent.KEYCODE_PERIOD -> "."
    AndroidKeyEvent.KEYCODE_SLASH -> "/"
    AndroidKeyEvent.KEYCODE_GRAVE -> "`"
    else -> null
}

/** The modifier keys themselves: a press / release of one alone is a `modifiers` change. */
private val MODIFIER_KEYS = mapOf(
    AndroidKeyEvent.KEYCODE_CTRL_LEFT to "ctrl", AndroidKeyEvent.KEYCODE_CTRL_RIGHT to "ctrl",
    AndroidKeyEvent.KEYCODE_ALT_LEFT to "alt", AndroidKeyEvent.KEYCODE_ALT_RIGHT to "alt",
    AndroidKeyEvent.KEYCODE_SHIFT_LEFT to "shift", AndroidKeyEvent.KEYCODE_SHIFT_RIGHT to "shift",
    AndroidKeyEvent.KEYCODE_META_LEFT to "meta", AndroidKeyEvent.KEYCODE_META_RIGHT to "meta",
)

/** The other side's meta-state bit of a modifier key (still held after this one's release). */
private fun otherSide(keyCode: Int): Int = when (keyCode) {
    AndroidKeyEvent.KEYCODE_CTRL_LEFT -> AndroidKeyEvent.META_CTRL_RIGHT_ON
    AndroidKeyEvent.KEYCODE_CTRL_RIGHT -> AndroidKeyEvent.META_CTRL_LEFT_ON
    AndroidKeyEvent.KEYCODE_ALT_LEFT -> AndroidKeyEvent.META_ALT_RIGHT_ON
    AndroidKeyEvent.KEYCODE_ALT_RIGHT -> AndroidKeyEvent.META_ALT_LEFT_ON
    AndroidKeyEvent.KEYCODE_SHIFT_LEFT -> AndroidKeyEvent.META_SHIFT_RIGHT_ON
    AndroidKeyEvent.KEYCODE_SHIFT_RIGHT -> AndroidKeyEvent.META_SHIFT_LEFT_ON
    AndroidKeyEvent.KEYCODE_META_LEFT -> AndroidKeyEvent.META_META_RIGHT_ON
    AndroidKeyEvent.KEYCODE_META_RIGHT -> AndroidKeyEvent.META_META_LEFT_ON
    else -> 0
}

/**
 * #EXP-1629: one Android key event while a KeyRecorder records → its
 * `record` payload, or null = send nothing (a key repeat, a key outside
 * the canonical list); the caller swallows the key either way. A modifier
 * key alone = `modifiers` with the flags AFTER the event (its own flag
 * from the event, the other side's bit kept); every other key = `down` /
 * `up` with its canonical name and the held flags. Pure (raw ints).
 */
fun keyRecorderKeyPayload(keyCode: Int, down: Boolean, metaState: Int, repeatCount: Int = 0): JsonValue? {
    if (repeatCount > 0) return null
    var ctrl = metaState and AndroidKeyEvent.META_CTRL_ON != 0
    var alt = metaState and AndroidKeyEvent.META_ALT_ON != 0
    var shift = metaState and AndroidKeyEvent.META_SHIFT_ON != 0
    var meta = metaState and AndroidKeyEvent.META_META_ON != 0
    val modifier = MODIFIER_KEYS[keyCode]
    if (modifier != null) {
        val held = down || metaState and otherSide(keyCode) != 0
        when (modifier) {
            "ctrl" -> ctrl = held
            "alt" -> alt = held
            "shift" -> shift = held
            else -> meta = held
        }
        return keyRecorderPayload("modifiers", ctrl, alt, shift, meta)
    }
    val key = keyRecorderKeyName(keyCode) ?: return null
    return keyRecorderPayload(if (down) "down" else "up", ctrl, alt, shift, meta, key = key)
}

/**
 * #EXP-1629: while the focused KeyRecorder field records, EVERY key is
 * its (Tab moves no focus, Escape closes nothing, no shortcut, no repeat):
 * the event is consumed and its payload, if any, goes out as `record`.
 * Null = not recording (the surface's keys as usual). The system Back of
 * the navigation bar (the virtual keyboard device) and a mouse's back
 * button (the mouse's button 4 is the pointer path) are left alone.
 */
private fun keyRecorderKey(model: SurfaceModel, e: KeyEvent): Boolean? {
    val n = model.keyboardFocus?.let(model::indexOf)?.let(model::node) ?: return null
    if (!n.isKeyRecorderField || !n.keyRecorderRecording) return null
    val native = e.nativeKeyEvent
    if (native.keyCode == AndroidKeyEvent.KEYCODE_BACK) {
        if (native.deviceId == KeyCharacterMap.VIRTUAL_KEYBOARD) return null
        if (native.source and InputDevice.SOURCE_MOUSE == InputDevice.SOURCE_MOUSE) return true
    }
    val down = when (e.type) {
        KeyEventType.KeyDown -> true
        KeyEventType.KeyUp -> false
        else -> return true
    }
    keyRecorderKeyPayload(native.keyCode, down, native.metaState, native.repeatCount)?.let { model.keyRecorderRecord(n.index, it) }
    return true
}

/**
 * VAPP-162 (round-5 §9): Space / Enter down and up on a focused `hold`
 * Button drive the core's hold (key repeat ignored there); Escape cancels a
 * held one (otherwise it closes layers as usual). Null = not a hold key.
 */
private fun holdKey(model: SurfaceModel, e: KeyEvent): Boolean? {
    val n = model.keyboardFocus?.let(model::indexOf)?.let(model::node) ?: return null
    if (!n.isHoldButton()) return null
    val key = when (e.key) {
        Key.Spacebar -> " "
        Key.Enter, Key.NumPadEnter -> "Enter"
        Key.Escape -> if (model.isHeld(n.id)) "Escape" else return null
        else -> return null
    }
    when (e.type) {
        KeyEventType.KeyDown -> model.holdInput(n.index, "keydown", key, repeat = e.nativeKeyEvent.repeatCount > 0)
        KeyEventType.KeyUp -> model.holdInput(n.index, "keyup", key)
        else -> return null
    }
    return true
}

/** One key on the focused node; `true` = handled. */
fun handleKey(model: SurfaceModel, key: String?, shift: Boolean = false, typed: String? = null): Boolean {
    val focused = model.keyboardFocus?.let(model::indexOf)?.let(model::node)
    if (key == "escape") {
        if (focused != null && focused.component == "Composer" && focused.props["busy"]?.bool == true) {
            model.fire(focused.index, "stop")
            return true
        }
        return model.escape()
    }
    val n = focused ?: return false
    if (key == "f10" && shift) {
        var p: Int? = n.index
        while (p != null) {
            val c = model.node(p) ?: break
            if (c.isContextMenu) {
                val f = model.frame(n.index)
                model.fire(c.index, "contextmenu", JsonValue.Obj(mapOf("x" to JsonValue.Num(f.center.x.toDouble()), "y" to JsonValue.Num(f.center.y.toDouble()))))
                return true
            }
            p = c.parent
        }
        return false
    }
    if (n.isTextField) return fieldKey(model, n, key)
    if (key == null) {
        if (typed != null && n.part == "item") return typeahead(model, n, typed)
        return false
    }
    // #VAPP-285: a Tree row (`treeitem`): the core's tree keys move the
    // focus (a `focus` event) or fold it.
    if ((n.accessibility?.get("role") as? JsonValue.Str)?.v == "treeitem") {
        val dom = when (key) {
            "up" -> "ArrowUp"
            "down" -> "ArrowDown"
            "home" -> "Home"
            "end" -> "End"
            "left" -> "ArrowLeft"
            "right" -> "ArrowRight"
            else -> null
        }
        if (dom != null) {
            model.nodeKey(n.index, dom)
            return true
        }
    }
    // #VAPP-284: a node that folds (a foldable Row's head or fold button:
    // `expanded` set): the core's fold keys decide (`fold_key`, rtl-aware).
    if ((key == "left" || key == "right") && n.accessibility?.get("expanded") is JsonValue.Bool) {
        model.nodeKey(n.index, if (key == "left") "ArrowLeft" else "ArrowRight")
        return true
    }
    val rtl = model.direction == "rtl"
    val h = arrowStep(key, rtl, vertical = false)
    val hv = arrowStep(key, rtl, vertical = true)
    val vertical = when (key) {
        "down" -> 1
        "up" -> -1
        else -> null
    }
    val owner = n.ownerComponent
    when {
        n.component == "Segmented" -> return segmentedKey(model, n, key, hv)
        n.component == "Slider" && n.part == "track" -> {
            val min = n.props.num("min") ?: 0.0
            val max = n.props.num("max") ?: 100.0
            val step = n.props.num("step") ?: 1.0
            val current = model.sliderValue(n.index)
            val delta = when (key) {
                "up" -> 1.0
                "down" -> -1.0
                "pageup" -> 10.0
                "pagedown" -> -10.0
                else -> h?.toDouble()
            }
            val value = when (key) {
                "home" -> min
                "end" -> max
                else -> SurfaceModel.snap(current + (delta ?: return false) * step, min, max, step)
            }
            model.fire(n.index, "change", JsonValue.Obj(mapOf("value" to JsonValue.Num(value))))
            return true
        }
        owner == "Carousel" && n.part == "indicator" -> {
            val count = (n.props.num("count") ?: 0.0).toInt()
            val page = (n.props.num("page") ?: 0.0).toInt()
            if (count <= 0) return false
            val next = when (key) {
                "home" -> 0
                "end" -> count - 1
                else -> Math.floorMod(page + (h ?: return false), count)
            }
            model.carouselPage(n.index, next)
            return true
        }
        owner == "Tabs" && n.part == "tab" -> roving(model, n, key, h)?.let { t ->
            model.focusRequest = model.node(t)?.id
            model.press(t)
            return true
        }
        owner == "Radio" && (n.part == "dot" || n.part == "item") -> roving(model, n, key, hv)?.let { t ->
            model.focusRequest = model.node(t)?.id
            model.press(t)
            return true
        }
        // EXP-1280: arrows rove the tiles (row-major); a single choice
        // follows focus like a Radio, a multiple one only moves it (Space /
        // Enter press the focused tile).
        owner == "ChoiceTiles" && n.part == "tile" -> roving(model, n, key, hv)?.let { t ->
            model.focusRequest = model.node(t)?.id
            if (model.ownerProps(n.index).str("type") != "multiple") model.press(t)
            return true
        }
        owner == "Accordion" && n.part == "trigger" -> roving(model, n, key, vertical)?.let { t ->
            model.focusRequest = model.node(t)?.id
            return true
        }
        owner == "Menu" && n.part == "item" -> {
            val kind = n.props["kind"]?.string ?: "item"
            val (openKey, closeKey) = if (rtl) "left" to "right" else "right" to "left"
            if (key == openKey && kind == "submenu") {
                model.press(n.index)
                return true
            }
            if (key == closeKey && n.layer > 1) {
                model.layers.firstOrNull { it.layer == n.layer }?.let { model.fire(it.root, "dismiss"); return true }
            }
            roving(model, n, key, vertical)?.let { t ->
                model.focusRequest = model.node(t)?.id
                return true
            }
        }
        (owner == "Select" || owner == "TimePicker") && n.part == "item" -> roving(model, n, key, vertical)?.let { t ->
            model.focusRequest = model.node(t)?.id
            // #EXP-1629: the arrows move the Select popup's cursor too.
            if (owner == "Select") n.owner?.let { o -> model.node(t)?.let { model.setSelectCursor(o, it.id) } }
            return true
        }
        (owner == "DatePicker" || owner == "DateRangePicker") && n.part == "day" -> return calendarKey(model, n, key, shift, h)
        owner == "Resizable" && n.part == "handle" -> {
            val dom = when (key) {
                "left" -> "ArrowLeft"
                "right" -> "ArrowRight"
                "up" -> "ArrowUp"
                "down" -> "ArrowDown"
                "home" -> "Home"
                "end" -> "End"
                "enter" -> "Enter"
                else -> return false
            }
            model.nodeKey(n.index, dom)
            return true
        }
    }
    if (key == "enter" || key == "space") {
        if (n.pressable || n.isPickerTrigger || n.triggerFor != null) {
            model.press(n.index)
            return true
        }
    }
    // Openers: ArrowDown (Up) on a picker / menu trigger opens it.
    if ((key == "down" || key == "up") && (n.triggerFor != null || n.part == "trigger") && owner != "Accordion" && owner != "Tooltip") {
        if (!n.open) model.press(n.index)
        return true
    }
    // A focused scroll container scrolls.
    model.scrolls[n.index]?.let { s ->
        val f = model.frame(n.index)
        val maxY = kotlin.math.max(0f, s.contentHeight - f.height)
        val y = when (key) {
            "down" -> s.offsetY + 40f
            "up" -> s.offsetY - 40f
            "pagedown", "space" -> s.offsetY + f.height * 0.9f
            "pageup" -> s.offsetY - f.height * 0.9f
            "home" -> 0f
            "end" -> maxY
            else -> return false
        }
        model.scrollTo(n.id, s.offsetX, y.coerceIn(0f, maxY))
        return true
    }
    return false
}

/** What a key means along an axis: +1 forward, −1 back (Left / Right swap in rtl); Up / Down too when [vertical]. */
internal fun arrowStep(key: String, rtl: Boolean, vertical: Boolean): Int? = when (key) {
    "right" -> if (rtl) -1 else 1
    "left" -> if (rtl) 1 else -1
    "down" -> if (vertical) 1 else null
    "up" -> if (vertical) -1 else null
    else -> null
}

/** The siblings a roving widget moves between: same owner + part, enabled, in pre-order. */
private fun rovingSet(model: SurfaceModel, n: NodeInfo): List<Int> =
    model.nodes.filter { it.owner == n.owner && it.part == n.part && !it.hidden && !model.isDisabled(it.index) && it.layer == n.layer }.map { it.index }

/** The next node of the roving set by `step` (wrapping); Home / End = the ends. */
private fun roving(model: SurfaceModel, n: NodeInfo, key: String, step: Int?): Int? {
    val set = rovingSet(model, n)
    if (set.isEmpty()) return null
    val at = set.indexOf(n.index).coerceAtLeast(0)
    return when (key) {
        "home" -> set.first()
        "end" -> set.last()
        else -> step?.let { set[Math.floorMod(at + it, set.size)] }
    }
}

/** Type-ahead in a listbox / menu: the next entry starting with `typed`. */
private fun typeahead(model: SurfaceModel, n: NodeInfo, typed: String): Boolean {
    val set = rovingSet(model, n)
    if (set.isEmpty()) return false
    val at = set.indexOf(n.index).coerceAtLeast(0)
    for (k in 1..set.size) {
        val i = set[(at + k) % set.size]
        val node = model.node(i) ?: continue
        val text = node.props["text"]?.displayText ?: model.nodes.firstOrNull { it.parent == i && it.props["text"] != null }?.props?.get("text")?.displayText ?: ""
        if (text.startsWith(typed, ignoreCase = true)) {
            model.focusRequest = node.id
            return true
        }
    }
    return false
}

/** Segmented: arrows move the roving item and select it (single), Space / Enter toggle it. */
private fun segmentedKey(model: SurfaceModel, n: NodeInfo, key: String, step: Int?): Boolean {
    val items = n.props["items"]?.array ?: return false
    if (items.isEmpty()) return false
    val values = model.segmentedValues(n.index)
    val at = items.indexOfFirst { values.contains((it["value"] ?: JsonValue.Null).displayText) }.coerceAtLeast(0)
    val next = when (key) {
        "home" -> 0
        "end" -> items.size - 1
        "enter", "space" -> at
        else -> Math.floorMod(at + (step ?: return false), items.size)
    }
    val item = items[next]
    model.segmentedSelect(n.index, item["value"] ?: JsonValue.Str(item["label"]?.displayText ?: ""))
    return true
}

/** Calendar grid keys: ±1 day / ±1 week (arrows), PageUp / PageDown month (Shift: year), Enter / Space pick. */
private fun calendarKey(model: SurfaceModel, n: NodeInfo, key: String, shift: Boolean, h: Int?): Boolean {
    val iso = n.props["date"]?.string ?: return false
    if (key == "enter" || key == "space") {
        model.press(n.index)
        return true
    }
    val date = runCatching { java.time.LocalDate.parse(iso) }.getOrNull() ?: return false
    val target = when (key) {
        "up" -> date.minusDays(7)
        "down" -> date.plusDays(7)
        "pageup" -> if (shift) date.minusYears(1) else date.minusMonths(1)
        "pagedown" -> if (shift) date.plusYears(1) else date.plusMonths(1)
        "home", "end" -> {
            val week = model.nodes.filter { it.parent == n.parent && it.part == "day" }
            (if (key == "home") week.firstOrNull() else week.lastOrNull())?.let { model.focusRequest = it.id; return true }
            return false
        }
        else -> date.plusDays((h ?: return false).toLong())
    }
    val t = target.toString()
    val day = model.nodes.firstOrNull { it.owner == n.owner && it.part == "day" && it.props["date"]?.string == t }
    if (day != null) {
        model.focusRequest = day.id
        return true
    }
    // Another month: page the calendar (its previous / next button), then focus there.
    val pager = model.nodes.firstOrNull { it.owner == n.owner && it.part == if (target.isBefore(date)) "previous" else "next" } ?: return false
    model.press(pager.index)
    model.nodes.firstOrNull { it.owner == n.owner && it.part == "day" && it.props["date"]?.string == t }?.let { model.focusRequest = it.id }
    return true
}

/** Keys inside a host text field: NumberField Up / Down step, ChipInput Backspace removes the last chip. */
private fun fieldKey(model: SurfaceModel, n: NodeInfo, key: String?): Boolean {
    // EXP-1259: a search list's field keeps the focus; ↑/↓ move the core's
    // highlighted row (Enter, Tab and ← at the caret's start come through
    // the field itself).
    if (n.component == "SearchList" && (key == "up" || key == "down")) {
        model.searchKey(n.index, if (key == "up") "ArrowUp" else "ArrowDown")
        return true
    }
    val owner = model.owner(n.index) ?: return false
    if (owner.component == "NumberField" && n.part == "input" && (key == "up" || key == "down")) {
        val part = if (key == "up") "increment" else "decrement"
        model.nodes.firstOrNull { it.owner == owner.id && it.part == part }?.let { model.press(it.index); return true }
    }
    if (owner.component == "ChipInput" && n.part == "input" && key == "backspace" && (n.props["text"]?.string ?: "").isEmpty()) {
        model.nodes.lastOrNull { it.owner == owner.id && it.part == "remove" }?.let { model.press(it.index); return true }
    }
    return false
}
