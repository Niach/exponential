package at.exponential.ui.model

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import at.exponential.ui.ffi.UiException
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.flag
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The host-owned text of one field (`Input`/`Textarea` `.field`, the
 * `Composer`). The CLIENT owns the string (a `BasicTextField` with its own
 * value, never a state the core re-writes per key: the VAPP-4 burst
 * finding); every edit carries a revision; a 150 ms debounce sends
 * `change`, blur / Enter send `commit`. An echo of the field's own edits
 * (a changed `value` prop from the host: updateDataModel, a bind) is
 * written in only while the field is idle and unfocused; a PROGRAMMATIC
 * write (an action's `set`: a press's `functionCall set`, a package
 * action's `sets` = `assignData`, both bump the surface's write epoch) is
 * adopted even while focused, an outstanding edit dropped (#VAPP-288).
 */
class FieldState internal constructor(
    val id: String,
    text: String,
    internal var external: JsonValue,
    /** The surface's write epoch the field last looked at. */
    internal var seenEpoch: ULong = 0u,
) {
    /** The field's text (what the view shows). */
    var text by mutableStateOf(text)
        internal set

    /** The revision of the last edit. */
    var revision = 0
        internal set

    /** Is the field focused? */
    var focused = false
        internal set

    internal var debounce: Job? = null

    /** An edit not yet sent as `change`. */
    var pendingChange = false
        internal set

    /** #EDE-65: an IME composition is in progress (the view's `TextFieldValue.composition`). */
    var composing = false
        internal set

    /** Bumped when the MODEL writes the text (the view reloads its value). */
    var writeGeneration by mutableIntStateOf(0)
        internal set
}

/** The debounce before a `change` goes out (ms). */
const val INPUT_DEBOUNCE_MS = 150L

/**
 * The value a field shows from the core: an inline field's (NumberField /
 * ChipInput `.input`, Select `.search`) own `text` prop (the core formats
 * the number, keeps the typed query), else the owner's `value`.
 */
internal fun SurfaceModel.fieldExternal(n: NodeInfo): JsonValue =
    if (n.isInlineField) n.props["text"] ?: JsonValue.Null else (owner(n.index) ?: n).props["value"] ?: JsonValue.Null

/** The field state of a text-field node, created from its props. */
fun SurfaceModel.field(index: Int): FieldState? {
    val n = node(index) ?: return null
    if (!n.isTextField) return null
    fieldsMap[n.id]?.let { return it }
    val external = fieldExternal(n)
    val f = FieldState(n.id, external.displayText, external, surface.writeEpoch())
    fieldsMap[n.id] = f
    return f
}

internal fun SurfaceModel.pruneFields() {
    val it = fieldsMap.entries.iterator()
    while (it.hasNext()) {
        val e = it.next()
        if (!byId.containsKey(e.key)) {
            e.value.debounce?.cancel()
            it.remove()
        }
    }
}

/** The field's text (what the view shows). */
fun SurfaceModel.fieldText(index: Int): String = field(index)?.text ?: ""

/** A platform edit: bump the revision, re-arm the debounce. */
fun SurfaceModel.fieldEdited(index: Int, text: String) {
    val f = field(index) ?: return
    f.text = text
    f.revision += 1
    f.pendingChange = true
    f.debounce?.cancel()
    f.debounce = scope.launch {
        delay(INPUT_DEBOUNCE_MS)
        flushField(f, index, commit = false)
    }
    // A field whose size follows its LIVE text (an inline field, an
    // autosize Textarea, the Composer) measures again now.
    val n = node(index) ?: return
    val grows = n.isInlineField || n.component == "Composer" || (n.component == "Textarea" && ownerProps(index).flag("autosize"))
    if (grows && surface.markDirty(index.toUInt())) {
        layoutNeeded = true
        pass()
    }
}

/**
 * Enter in a single-line field (round 1 §3): the pending text goes out as
 * `submit` (a Form submits; a ChipInput adds the chip and empties).
 */
fun SurfaceModel.fieldSubmitted(index: Int) {
    val f = field(index) ?: return
    val n = node(index) ?: return
    // EXP-1259: Enter in a search list's field picks its highlighted row.
    if (n.component == "SearchList") return searchKey(index, "Enter")
    f.debounce?.cancel()
    f.debounce = null
    f.pendingChange = false
    val events = try {
        surface.event(index.toUInt(), "submit", JsonValue.Obj(mapOf("value" to JsonValue.Str(f.text))).json)
    } catch (_: UiException) {
        return
    }
    f.external = JsonValue.Str(f.text)
    if (n.component == "ChipInput") {
        f.text = ""
        f.external = JsonValue.Str("")
        f.writeGeneration += 1
    }
    dispatch(events, inputRevision = f.revision)
}

/**
 * EXP-1259: a key for a search list's field (`ArrowDown`, `ArrowUp`,
 * `Enter`, `Tab`, `ArrowLeft` = leave a searchable submenu): the pending
 * text goes first (the filter is current), then the core moves or picks the
 * highlighted row.
 */
fun SurfaceModel.searchKey(index: Int, key: String) {
    field(index)?.let { f ->
        f.debounce?.cancel()
        flushField(f, index, commit = false)
    }
    fire(index, "key", JsonValue.Obj(mapOf("key" to JsonValue.Str(key))))
}

/** Blur / Enter: send what is pending as a `commit`. */
fun SurfaceModel.fieldCommitted(index: Int) {
    val f = field(index) ?: return
    f.debounce?.cancel()
    f.debounce = null
    flushField(f, index, commit = true)
}

/** Focus moved into / out of the field (out = a commit). */
fun SurfaceModel.fieldFocused(index: Int, focused: Boolean) {
    val f = field(index) ?: return
    val n = node(index) ?: return
    f.focused = focused
    focusedField = if (focused) n.id else if (focusedField == n.id) null else focusedField
    focus(n.id, focused)
    if (!focused) {
        fieldCommitted(index)
        // The core's blur (`validateOn: blur` checks, a NumberField's clamp).
        fire(index, "blur")
    }
}

private fun SurfaceModel.flushField(f: FieldState, index: Int, commit: Boolean) {
    if (!commit && !f.pendingChange) return
    f.pendingChange = false
    f.debounce = null
    val name = if (commit) "commit" else "change"
    val events = try {
        surface.event(index.toUInt(), name, JsonValue.Obj(mapOf("value" to JsonValue.Str(f.text))).json)
    } catch (_: UiException) {
        return
    }
    // The write-through changed the prop the field mirrors: take it as the
    // new external value so the echo of our own edit never applies.
    f.external = JsonValue.Str(f.text)
    dispatch(events, inputRevision = f.revision)
}

/** Composer Enter / send: `submit` (or `stop` while busy), then the field empties. */
fun SurfaceModel.composerSubmit(index: Int) {
    val f = field(index) ?: return
    val n = node(index) ?: return
    if (n.props.flag("busy")) {
        fire(index, "stop")
        return
    }
    // #EDE-65 (`composerKeys`): the trimmed text goes out.
    val text = f.text.trim()
    if (text.isEmpty()) return
    val rev = f.revision
    f.debounce?.cancel()
    f.debounce = null
    f.pendingChange = false
    f.text = ""
    f.writeGeneration += 1
    f.external = JsonValue.Str("")
    val events = try {
        surface.event(index.toUInt(), "submit", JsonValue.Obj(mapOf("value" to JsonValue.Str(text))).json)
    } catch (_: UiException) {
        return
    }
    dispatch(events, inputRevision = rev)
    // The emptied draft writes through (gpui `submit_composer`, Swift's
    // `composerSubmit`), so a bound value reads "" after a send.
    val cleared = try {
        surface.event(index.toUInt(), "change", JsonValue.Obj(mapOf("value" to JsonValue.Str(""))).json)
    } catch (_: UiException) {
        return
    }
    dispatch(cleared, inputRevision = f.revision)
}

/**
 * #EDE-65 (`interactions-round1.json` `composerKeys`): the `key` payload
 * name of a key in a Composer with menuKeys on (`up`, `down`, `enter`,
 * `tab`, `escape`), or null when the field keeps it (any modifier held:
 * Shift+Enter's newline, Shift+Tab, Alt+Down). Pure.
 */
fun composerMenuKey(key: String, modified: Boolean): String? = when {
    modified -> null
    key == "up" || key == "down" || key == "enter" || key == "tab" || key == "escape" -> key
    else -> null
}

/** #EDE-65: does Composer [index] take the menu keys (its `menuKeys`, read live)? */
fun SurfaceModel.composerMenuKeysOn(index: Int): Boolean {
    val n = node(index) ?: return false
    return n.component == "Composer" && n.isTextField && n.props.flag("menuKeys")
}

/**
 * #EDE-65: a key down in Composer [index] (a canonical name; [modified] =
 * shift / ctrl / alt / meta held, [composing] = an IME composition is in
 * progress). With menuKeys on, a menu key sends the pending text first
 * (the host's filter is current), then fires `key` {key}; `true` = taken
 * (the field does not edit, move its caret, submit or move the focus).
 * Off, composing or modified = `false` (the field's own key: Enter submits).
 */
fun SurfaceModel.composerKey(index: Int, key: String, modified: Boolean = false, composing: Boolean = false): Boolean {
    if (composing || !composerMenuKeysOn(index)) return false
    val name = composerMenuKey(key, modified) ?: return false
    field(index)?.let { f ->
        f.debounce?.cancel()
        flushField(f, index, commit = false)
    }
    fire(index, "key", JsonValue.Obj(mapOf("key" to JsonValue.Str(name))))
    return true
}

/**
 * A press sends the focused field's outstanding edit first (a `change`), so
 * the action sees the current text and a programmatic `set` back to the
 * pre-edit value still reads as a change of the field's value.
 */
internal fun SurfaceModel.flushFocusedField() {
    val id = focusedField ?: return
    val f = fieldsMap[id] ?: return
    if (!f.pendingChange) return
    val index = indexOf(id) ?: return
    f.debounce?.cancel()
    flushField(f, index, commit = false)
}

/**
 * After a pass: apply changed `value` props. An echo lands only in idle,
 * unfocused fields; a programmatic write (the write epoch moved and the
 * field's value changed) lands even while focused (#VAPP-288).
 */
internal fun SurfaceModel.echoFields() {
    val epoch = runCatching { surface.writeEpoch() }.getOrDefault(0uL)
    for (n in nodes) {
        if (!n.isTextField) continue
        val f = fieldsMap[n.id] ?: continue
        val external = fieldExternal(n)
        val programmatic = epoch != f.seenEpoch
        f.seenEpoch = epoch
        if (external == f.external) continue
        f.external = external
        val text = external.displayText
        if (programmatic && (f.focused || f.pendingChange)) {
            f.debounce?.cancel()
            f.debounce = null
            f.pendingChange = false
            if (text != f.text) {
                f.text = text
                f.writeGeneration += 1
            }
            continue
        }
        if (f.focused || f.pendingChange) continue
        if (text != f.text) {
            f.text = text
            f.writeGeneration += 1
        }
    }
}
