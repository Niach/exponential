package com.exponential.app.data.composer

import com.exponential.app.domain.ComposerDraft
import com.exponential.app.domain.ComposerSend
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

/**
 * #FEED-219 (fixture `composer-draft.json`): binds ONE composer field (a
 * ViewModel's or a connection's draft flow) to the synced draft of its
 * target, the Android twin of web `hooks/use-composer-draft.ts`.
 *
 * Until the person edits the field it FOLLOWS the target's draft
 * ([ComposerDraft.field]: the unlanded local write, else the synced row, else
 * empty); every change the binding did not make itself is an edit, written
 * debounced. A new target flushes the old one and starts a new, untouched
 * field. The field's own flow stays the composer's one source of truth, so
 * the screens (and `ComposerDraftSync`) are unchanged: no new UI.
 */
class ComposerDraftBinding(
    private val scope: CoroutineScope,
    private val writer: ComposerDraftWriter,
    /** What an untouched field shows for a key ([ComposerDrafts.shown]). */
    private val shown: (ComposerDraftWriter.Id) -> Flow<String>,
    private val field: StateFlow<String>,
    private val setField: (String) -> Unit,
) {
    constructor(
        scope: CoroutineScope,
        drafts: ComposerDrafts,
        field: StateFlow<String>,
        setField: (String) -> Unit,
    ) : this(scope, drafts.writer, drafts::shown, field, setField)

    /** The bound target: the row's key + the team it files under. */
    data class Bound(val id: ComposerDraftWriter.Id, val teamId: String)

    var bound: Bound? = null
        private set

    private var edited = false

    /** The last text the binding wrote into the field or the writer. */
    private var known: String = field.value
    private var follow: Job? = null

    init {
        scope.launch { field.collect { onField(it) } }
    }

    private fun onField(text: String) {
        if (text == known) return
        known = text
        val at = bound ?: return
        edited = true
        writer.write(at.id, at.teamId, text)
    }

    /**
     * The field's target now; null [accountId], [target] or [teamId] = a
     * plain field (the team not resolved yet, an edit composer). Re-binding
     * the same target is a no-op.
     */
    fun bind(accountId: String?, target: ComposerDraft.Target?, teamId: String?) {
        val next = if (accountId != null && target != null && !teamId.isNullOrEmpty()) {
            Bound(ComposerDraftWriter.Id(accountId, target.key), teamId)
        } else {
            null
        }
        if (next?.id == bound?.id) {
            // Same key: only the team may have moved (a draft files there).
            bound = next
            return
        }
        val previous = bound
        previous?.let { writer.flush(it.id) }
        follow?.cancel()
        follow = null
        bound = next
        edited = false
        if (next == null) {
            // Leaving a target for none (another issue whose team is not
            // resolved yet): the old target's text must not carry over.
            if (previous != null) adopt("")
            return
        }
        // Text typed before the FIRST binding (the team resolving late, a
        // connection that outlived its screen) is the person's unless the
        // draft already says the same; a switch from another target starts
        // a new field at once (no frame of the old target's text).
        val typed = if (previous == null && field.value.isNotBlank()) field.value else null
        if (typed == null) adopt(writer.local(next.id) ?: "")
        follow = scope.launch {
            var first = true
            shown(next.id).collect { text ->
                if (first && typed != null && text != typed && !edited) {
                    edited = true
                    writer.write(next.id, next.teamId, field.value)
                }
                first = false
                if (!edited && bound == next) adopt(text)
            }
        }
    }

    private fun adopt(text: String) {
        known = text
        if (field.value != text) setField(text)
    }

    /** Write the field now (a send settled into it, the composer folds). */
    fun writeNow() {
        val at = bound ?: return
        known = field.value
        writer.write(at.id, at.teamId, field.value, now = true)
    }

    /**
     * A send that went out while bound to [at] settled (composer-send.json
     * already applied to the field when it is still [at]'s): write the field,
     * or, if the composer moved to another target meanwhile, clear [at]'s
     * draft when it still holds what was sent.
     */
    fun settleSent(at: Bound?, sentText: String) {
        if (at == null) return
        if (at.id == bound?.id) {
            writeNow()
            return
        }
        val stored = writer.local(at.id) ?: sentText
        writer.write(at.id, at.teamId, ComposerSend.textAfterSend(sentText, stored), now = true)
    }

    /** Discard the draft (a reply's Cancel): empties the field and deletes the row. */
    fun discard() {
        val at = bound ?: run { setField(""); return }
        known = ""
        edited = true
        setField("")
        writer.write(at.id, at.teamId, "", now = true)
    }

    /** The composer goes away: write what is pending now. */
    fun flush() {
        bound?.let { writer.flush(it.id) }
    }
}
