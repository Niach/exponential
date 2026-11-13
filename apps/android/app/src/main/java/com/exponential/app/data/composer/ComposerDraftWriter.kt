package com.exponential.app.data.composer

import com.exponential.app.domain.ComposerDraft
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * #FEED-219 (fixture `composer-draft.json`): the writer for composer drafts,
 * the Android twin of web `lib/composer-drafts.ts`. Writes are DEBOUNCED per
 * (account, key) ([ComposerDraft.DEBOUNCE_MS] after the last keystroke) and
 * FLUSHED at once when a composer goes away, after a send settles and when
 * the app goes to the background. Until a write has landed, its text is the
 * key's LOCAL copy ([local]), so a composer that remounts in between never
 * shows an older synced row. A failed write is silent: the local copy stays
 * and the next write retries.
 *
 * Every call runs on [scope]'s (single, main) thread; no locks.
 */
class ComposerDraftWriter(
    private val scope: CoroutineScope,
    private val transport: Transport,
    private val debounceMs: Long = ComposerDraft.DEBOUNCE_MS,
) {
    interface Transport {
        /** Upsert the row (and mirror it locally, so the synced copy matches). */
        suspend fun upsert(accountId: String, teamId: String, key: String, text: String)

        /** Delete the row (and locally). */
        suspend fun delete(accountId: String, key: String)
    }

    data class Id(val accountId: String, val key: String)

    private class Entry(var teamId: String, var text: String) {
        var timer: Job? = null
        var writing = false
        var dirty = false
    }

    private val entries = mutableMapOf<Id, Entry>()
    private val locals = MutableStateFlow<Map<Id, String>>(emptyMap())

    private fun publish() {
        locals.value = entries.mapValues { it.value.text }
    }

    /** The key's own write not yet landed (null = nothing pending). */
    fun local(id: Id): String? = entries[id]?.text

    fun localFlow(id: Id): Flow<String?> = locals.map { it[id] }.distinctUntilChanged()

    /** Record the field's text; it is written [debounceMs] later, or at once. */
    fun write(id: Id, teamId: String, text: String, now: Boolean = false) {
        val entry = entries.getOrPut(id) { Entry(teamId, text) }
        entry.teamId = teamId
        entry.text = text
        entry.dirty = true
        publish()
        entry.timer?.cancel()
        entry.timer = null
        if (now) {
            flush(id)
            return
        }
        entry.timer = scope.launch {
            delay(debounceMs)
            entry.timer = null
            flushNow(id)
        }
    }

    /** Write the key's pending text now (no-op when nothing is due). */
    fun flush(id: Id) {
        val entry = entries[id] ?: return
        entry.timer?.cancel()
        entry.timer = null
        if (entry.writing || !entry.dirty) return
        scope.launch { flushNow(id) }
    }

    /** Write every pending draft now (the app is going to the background). */
    fun flushAll() {
        for (id in entries.keys.toList()) flush(id)
    }

    private suspend fun flushNow(id: Id) {
        val entry = entries[id] ?: return
        entry.timer?.cancel()
        entry.timer = null
        if (entry.writing || !entry.dirty) return
        val text = entry.text
        entry.dirty = false
        entry.writing = true
        try {
            when (val write = ComposerDraft.write(text)) {
                is ComposerDraft.Write.Upsert -> transport.upsert(id.accountId, entry.teamId, id.key, write.text)
                ComposerDraft.Write.Delete -> transport.delete(id.accountId, id.key)
            }
        } catch (cancel: CancellationException) {
            entry.dirty = true
            throw cancel
        } catch (_: Throwable) {
            // Silent: the local copy stays, the next write retries.
            entry.dirty = true
        } finally {
            entry.writing = false
        }
        if (entries[id] !== entry) return
        if (entry.dirty && entry.text != text) {
            // Typed while the write was in flight: write the newer text.
            if (entry.timer == null) flushNow(id)
            return
        }
        if (!entry.dirty && entry.timer == null) {
            entries.remove(id)
            publish()
        }
    }
}
