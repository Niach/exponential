package com.exponential.app.ui.components

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import kotlin.reflect.KProperty
import kotlinx.coroutines.flow.StateFlow

/**
 * EXP-1520: a composer field whose text also lives in a draft store (the steer
 * connection's draft survives a reconnect; the launch page's survives the
 * page). The FIELD is the authority while the user types. The store only ever
 * wins on an OUTSIDE rewrite: a send clearing it, a `/` pick, a prompt
 * insert, a restored draft.
 *
 * The screens used to reconcile in `LaunchedEffect(draft)` keyed on the
 * COLLECTED draft. That value trails the field by at least a frame, so a
 * keystroke landing in between was overwritten by the stale echo and then
 * restored by the next one. Each round trip also dropped the IME's composing
 * region, which on a busy phone read as dropped, reordered or doubled letters.
 * [adoptStored] compares the field with the store's CURRENT value instead: the
 * screens write the store synchronously on every edit, so the two only differ
 * after an outside rewrite.
 *
 * `var composerField by sync` reads and writes the field (a snapshot state).
 */
@Stable
class ComposerDraftSync(private val store: StateFlow<String>) {
    var field by mutableStateOf(store.value.let { TextFieldValue(it, TextRange(it.length)) })

    operator fun getValue(thisRef: Any?, property: KProperty<*>): TextFieldValue = field

    operator fun setValue(thisRef: Any?, property: KProperty<*>, value: TextFieldValue) {
        field = value
    }

    /**
     * Takes the store's current text when it differs from the field (an
     * outside rewrite). The selection is clamped into the new text, and the
     * composition is dropped on purpose because it indexes characters that are
     * gone. Returns whether the field was rewritten.
     */
    fun adoptStored(): Boolean {
        val stored = store.value
        val current = field
        if (current.text == stored) return false
        field = TextFieldValue(
            stored,
            TextRange(
                current.selection.start.coerceIn(0, stored.length),
                current.selection.end.coerceIn(0, stored.length),
            ),
        )
        return true
    }

    /** Follows the store for the field's lifetime; [onOutsideRewrite] runs after each adopted rewrite. */
    suspend fun follow(onOutsideRewrite: () -> Unit) {
        store.collect { if (adoptStored()) onOutsideRewrite() }
    }
}
