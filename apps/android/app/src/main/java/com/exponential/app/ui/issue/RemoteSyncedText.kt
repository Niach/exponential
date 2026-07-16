package com.exponential.app.ui.issue

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/**
 * Reconciles a locally-edited text field with the remote value arriving through
 * Electric. One instance per field (title, description). Mirrors the iOS
 * IssueEditorModel reconciliation (live-apply when clean, stash + reload banner
 * when dirty, field-level last-write-wins while dirty).
 *
 * Same @Stable snapshot-state pattern as EditorModel — mutations are plain logic,
 * so this exercises fine as a plain-JVM unit test.
 */
@Stable
class RemoteSyncedText(
    // Save-form normalizer for echo detection: the server persists a transformed
    // form of the field (trimmed title / draft-image-stripped description), so
    // the Electric echo of our own save must be recognized through it.
    private val normalizeForEcho: (String) -> String = { it },
) {
    var text by mutableStateOf("")
        private set
    var pendingRemote by mutableStateOf<String?>(null)
        private set

    // Null until the first remote value seeds it. `focused` is a plain var — only
    // read/written from Compose effects and focus handlers (single-threaded UI).
    private var baseline: String? = null
    private var focused = false

    val isDirty: Boolean get() = baseline != null && text != baseline

    fun onUserEdit(new: String) {
        text = new
    }

    fun setFocused(nowFocused: Boolean) {
        focused = nowFocused
        // Blurring with clean text: applying the stash loses nothing, so do it.
        // This is a deliberate small superset of iOS (reachable state: focused
        // but untyped when the remote edit arrived).
        if (!nowFocused && !isDirty) pendingRemote?.let { seed(it) }
    }

    fun syncRemote(remote: String) {
        val base = baseline
        when {
            // First load: adopt the remote value as the baseline.
            base == null -> seed(remote)
            // The row re-emitted with no change to this column (another column
            // moved) — no divergence, drop any stale stash so no false banner.
            remote == base -> pendingRemote = null
            // Echo of our own save: the persisted form (trimmed / draft-stripped)
            // equals the remote. Re-baseline WITHOUT touching `text` — a still
            // in-flight draft image stays in the editor and keeps the field dirty
            // by byte-inequality, so a foreign remote arriving mid-upload stashes
            // instead of wiping the pending image.
            normalizeForEcho(text) == remote -> {
                baseline = remote
                pendingRemote = null
            }
            // Clean (BYTE equality — never normalized, which is what protects an
            // in-flight draft image from a live apply) and not focused: live-apply.
            !focused && text == base -> seed(remote)
            // Dirty or focused: stash for the reload banner (last-write-wins until
            // the user reloads — the local save still overwrites remote meanwhile).
            else -> pendingRemote = remote
        }
    }

    /** Banner tap: discard local text, take the stashed remote value. */
    fun reloadPending(): Boolean {
        val pending = pendingRemote ?: return false
        seed(pending)
        return true
    }

    private fun seed(value: String) {
        text = value
        baseline = value
        pendingRemote = null
    }
}
