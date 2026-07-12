package com.exponential.app.ui.markdown

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.PendingImage
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope

/**
 * Single source of truth for the block markdown editor — the Compose analog of
 * iOS `IssueEditorModel`. Owns the flat [EditorRow] document and routes every
 * edit through intent methods; markdown is derived only at save
 * ([currentMarkdown]), never per keystroke.
 *
 * Revisions are bumped ONLY for structural / external changes (load, split,
 * merge, insert, delete) so a field never clobbers the characters the user just
 * typed — mirroring the iOS revision discipline.
 */
@Stable
class EditorModel {

    enum class ImageUploadState { Idle, Uploading, Failed }

    var rows by mutableStateOf<List<EditorRow>>(
        listOf(EditorRow.Para(text = "", attrs = ParagraphAttrs.PLAIN, marks = emptyList())),
    )
        private set

    val pendingImages = mutableStateMapOf<String, PendingImage>()
    val uploadStates = mutableStateMapOf<String, ImageUploadState>()

    // Why the last upload for a row failed (server message like "Unsupported
    // image type" or "Images must be 10 MB or smaller"). Shown on the Failed
    // overlay — a bare "tap to retry" hid deterministic 4xx rejections and made
    // failures undiagnosable (EXP-61).
    val uploadErrors = mutableStateMapOf<String, String>()

    var focusedRowId by mutableStateOf<String?>(null)
        private set

    // Observable so the toolbar recomposes on caret/selection changes (active
    // mark highlight, enable-on-selection) — parity with iOS updateState().
    private var selection by mutableStateOf<Pair<String, IntRange>?>(null)

    // Inline marks queued to apply to the NEXT characters typed at a collapsed
    // caret — the Compose analog of iOS `typingAttributes`. Anchored to a
    // (rowId, caret) so any caret move or non-insertion edit drops the queue.
    // Observable so the toolbar reflects the pending state in its active tint.
    var pendingMarks by mutableStateOf<Set<InlineKind>>(emptySet())
        private set
    private var pendingAnchor: Pair<String, Int>? = null

    /** The row the toolbar should act on (focused, or last-selected). */
    val activeRowId: String? get() = focusedRowId ?: selection?.first

    /** The active row's selection range (caret if collapsed), for toolbar mark ops. */
    fun activeSelection(): Pair<String, IntRange>? {
        val rid = activeRowId ?: return null
        val sel = selection?.takeIf { it.first == rid } ?: return rid to (0..0)
        return rid to sel.second
    }

    var desiredSelection by mutableStateOf<Pair<String, Int>?>(null)
        private set

    private val revisions = mutableStateMapOf<String, Int>()
    private var revCounter = 0

    var lastSavedMarkdown by mutableStateOf("")
        private set

    var onEdit: (() -> Unit)? = null

    val isEditing: Boolean get() = focusedRowId != null
    fun currentMarkdown(): String = EditorRows.toMarkdown(rows)
    val isDirty: Boolean get() = currentMarkdown() != lastSavedMarkdown
    val hasUncommittedDrafts: Boolean get() = hasDraftImages(currentMarkdown())

    fun revision(id: String): Int = revisions[id] ?: 0
    fun uploadState(id: String): ImageUploadState = uploadStates[id] ?: ImageUploadState.Idle
    fun uploadError(id: String): String? = uploadErrors[id]

    // -- Loading ----------------------------------------------------------------

    fun load(markdown: String) {
        pendingImages.clear()
        uploadStates.clear()
        uploadErrors.clear()
        uploaders.clear()
        rows = EditorRows.fromBlocks(MarkdownParser.parse(markdown))
        bumpAll()
        // Baseline against the DERIVED markdown — the round-trip is not
        // byte-identical, so using the raw input would read as instantly dirty.
        lastSavedMarkdown = currentMarkdown()
        focusedRowId = null
        selection = null
    }

    fun markSaved(markdown: String) {
        lastSavedMarkdown = markdown
    }

    // -- Focus / selection ------------------------------------------------------

    fun setFocused(id: String?) {
        focusedRowId = id
    }

    fun clearFocusIfMatches(id: String) {
        if (focusedRowId == id) focusedRowId = null
    }

    fun updateSelection(rowId: String, range: IntRange) {
        selection = rowId to range
        // Drop queued pending marks once the caret leaves the anchored collapsed
        // position (moved, selected a range, or switched rows).
        val anchor = pendingAnchor ?: return
        val collapsedAt = if (range.first == range.last) rowId to range.first else null
        if (collapsedAt != anchor) {
            pendingMarks = emptySet()
            pendingAnchor = null
        }
    }

    fun consumeDesiredSelection(id: String): Int? {
        val ds = desiredSelection ?: return null
        if (ds.first != id) return null
        desiredSelection = null
        return ds.second
    }

    // -- Text editing -----------------------------------------------------------

    /** Non-structural intra-paragraph edit: update text + remap marks. No revision bump. */
    fun updatePara(rowId: String, newText: String, caret: Int) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.Para ?: return
        if (row.text == newText) return
        val remapped = MarkRemap.remap(row.text, newText, row.marks)
        val finalMarks = applyPendingMarks(rowId, row.text, newText, caret, remapped)
        rows = rows.toMutableList().also { it[idx] = row.copy(text = newText, marks = finalMarks) }
        selection = rowId to (caret..caret)
        notifyEdit()
    }

    /**
     * If marks are queued (via [togglePendingMark]) and this edit is a pure
     * single-run insertion at the anchored caret, wrap the inserted range with
     * each queued kind and re-anchor so consecutive typing keeps inheriting.
     * Any other edit (deletion, replace, paste, insert elsewhere) clears the
     * queue. Returns the marks to store on the row.
     */
    private fun applyPendingMarks(
        rowId: String,
        oldText: String,
        newText: String,
        caret: Int,
        remapped: List<com.exponential.app.ui.markdown.model.InlineMark>,
    ): List<com.exponential.app.ui.markdown.model.InlineMark> {
        if (pendingMarks.isEmpty()) return remapped
        val anchor = pendingAnchor
        val insLen = newText.length - oldText.length
        val insStart = caret - insLen
        val isPureInsertAtAnchor = anchor != null && anchor.first == rowId &&
            anchor.second == insStart && insLen > 0 &&
            insStart in 0..oldText.length && caret in insStart..newText.length &&
            newText.removeRange(insStart, caret) == oldText
        if (!isPureInsertAtAnchor) {
            pendingMarks = emptySet()
            pendingAnchor = null
            return remapped
        }
        var out = remapped
        for (kind in pendingMarks) out = MarkOps.addMark(out, insStart, caret, kind)
        pendingAnchor = rowId to caret
        return out
    }

    /** Enter: split the paragraph at [caret] (or list-continue / list-exit). */
    /**
     * Handle a newline-bearing edit (Enter or a multi-line paste). [fullText] is
     * the field's POST-EDIT text — Compose has already deleted any selected range
     * and inserted the newline(s) — so we split on it directly, which makes
     * Enter-over-a-selection replace the selection (not duplicate it) and a paste
     * of `A\nB\nC` become three rows with nothing dropped.
     */
    fun splitParagraphFrom(rowId: String, fullText: String) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.Para ?: return
        val parts = fullText.split("\n")

        // Empty list item with a lone Enter → exit the list instead of adding one.
        if (parts.size == 2 && parts[0].isEmpty() && parts[1].isEmpty() &&
            row.attrs.kind == BlockKind.ListItem
        ) {
            replaceRow(idx, row.copy(text = "", marks = emptyList(), attrs = ParagraphAttrs.PLAIN))
            bump(row.id)
            desiredSelection = row.id to 0
            renumberOrdered()
            notifyEdit()
            return
        }

        // First line keeps the row's block attrs; its marks are the original row's
        // marks remapped onto the (possibly shortened) first-line text.
        val firstText = parts.first()
        val first = row.copy(text = firstText, marks = MarkRemap.remap(row.text, firstText, row.marks))

        // Continuation lines: lists continue (ordered increments, checklist resets),
        // code/quote continue, everything else becomes a plain paragraph.
        val contAttrs = when (row.attrs.kind) {
            BlockKind.ListItem -> when (row.attrs.listType) {
                ListType.Checklist -> row.attrs.copy(checked = false)
                else -> row.attrs
            }
            BlockKind.CodeBlock, BlockKind.Blockquote -> row.attrs
            else -> ParagraphAttrs.PLAIN
        }
        val rest = parts.drop(1).map {
            EditorRow.Para(text = it, attrs = contAttrs, marks = emptyList())
        }

        val next = rows.toMutableList()
        next[idx] = first
        next.addAll(idx + 1, rest)
        rows = next
        bump(first.id)
        rest.forEach { bump(it.id) }
        val last = rest.last()
        focusedRowId = last.id
        desiredSelection = last.id to 0
        renumberOrdered()
        notifyEdit()
    }

    /** Backspace at offset 0: clear paragraph formatting, then merge / delete image. */
    fun backspaceAtStart(rowId: String) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.Para ?: return

        // First press on a formatted line just clears its block formatting.
        if (row.attrs.kind != BlockKind.Paragraph) {
            replaceRow(idx, row.copy(attrs = ParagraphAttrs.PLAIN))
            bump(row.id)
            desiredSelection = row.id to 0
            renumberOrdered()
            notifyEdit()
            return
        }

        if (idx == 0) return
        val prev = rows[idx - 1]
        when (prev) {
            is EditorRow.Image -> {
                // Delete the image above, then merge this para into the para above it.
                dropPendingDraft(prev)
                uploadStates.remove(prev.id)
                uploaders.remove(prev.id)
                val prevPrevIdx = idx - 2
                val next = rows.toMutableList()
                if (prevPrevIdx >= 0 && next[prevPrevIdx] is EditorRow.Para) {
                    val target = next[prevPrevIdx] as EditorRow.Para
                    val mergePoint = target.text.length
                    val merged = target.copy(
                        text = target.text + row.text,
                        marks = target.marks + MarkOps.offset(row.marks, mergePoint),
                    )
                    next[prevPrevIdx] = merged
                    next.removeAt(idx)        // current
                    next.removeAt(idx - 1)    // image
                    rows = EditorRows.normalize(next)
                    bumpAll()
                    focusedRowId = merged.id
                    desiredSelection = merged.id to mergePoint
                } else {
                    next.removeAt(idx - 1)    // just remove the image
                    rows = EditorRows.normalize(next)
                    bumpAll()
                    focusedRowId = row.id
                    desiredSelection = row.id to 0
                }
                renumberOrdered()
                notifyEdit()
            }
            is EditorRow.Para -> {
                val mergePoint = prev.text.length
                val merged = prev.copy(
                    text = prev.text + row.text,
                    marks = prev.marks + MarkOps.offset(row.marks, mergePoint),
                )
                val next = rows.toMutableList()
                next[idx - 1] = merged
                next.removeAt(idx)
                rows = next
                bump(merged.id)
                focusedRowId = merged.id
                desiredSelection = merged.id to mergePoint
                renumberOrdered()
                notifyEdit()
            }
        }
    }

    // -- Inline marks -----------------------------------------------------------

    fun toggleMark(rowId: String, range: IntRange, kind: InlineKind, href: String? = null) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.Para ?: return
        val start = range.first.coerceIn(0, row.text.length)
        val end = (range.last).coerceIn(start, row.text.length)
        if (kind == InlineKind.Link) {
            // Link is set/replaced (never "toggled off" by range here).
            val cleared = MarkOps.removeMark(row.marks, start, end, InlineKind.Link)
            val updated = if (href.isNullOrBlank()) cleared
            else MarkOps.addMark(cleared, start, end, InlineKind.Link, href)
            replaceRow(idx, row.copy(marks = updated))
            setFocused(rowId)
            notifyEdit()
            return
        }
        if (end <= start) return
        val has = MarkOps.hasMarkOver(row.marks, start, end, kind)
        val updated = if (has) {
            MarkOps.removeMark(row.marks, start, end, kind)
        } else {
            MarkOps.addMark(row.marks, start, end, kind)
        }
        replaceRow(idx, row.copy(marks = updated))
        // Re-assert focus: tapping a toolbar button took OS focus off the field
        // (clearing focusedRowId), and without this the keyboard dismisses and the
        // selection highlight is lost, so a second format tap can't act. The
        // field keeps its remembered TextFieldValue selection (no revision bump),
        // so the range is preserved.
        setFocused(rowId)
        notifyEdit()
    }

    /**
     * Queue (or unqueue) an inline mark to apply to the next characters typed at
     * a collapsed caret — used by the toolbar when there is no selection, so Bold
     * / Italic stay tappable and affect what you type next (iOS typingAttributes
     * parity). With a real selection the toolbar uses [toggleMark] instead.
     */
    fun togglePendingMark(rowId: String, caret: Int, kind: InlineKind) {
        if (pendingAnchor != rowId to caret) {
            pendingMarks = emptySet()
            pendingAnchor = rowId to caret
        }
        pendingMarks = pendingMarks.toMutableSet().apply { if (!add(kind)) remove(kind) }
        // Re-assert focus + caret: the toolbar tap took OS focus off the field
        // (clearing focusedRowId); without this the keyboard dismisses. Mirrors
        // the rationale in [toggleMark]. The anchor is set above so this caret
        // re-assert doesn't clear the queue we just built.
        setFocused(rowId)
        updateSelection(rowId, caret..caret)
    }

    fun pendingMarkActive(rowId: String, caret: Int, kind: InlineKind): Boolean =
        pendingAnchor == rowId to caret && kind in pendingMarks

    /** Insert link display text at [at] and mark it (used when there is no selection). */
    fun insertLinkText(rowId: String, at: Int, text: String, url: String) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.Para ?: return
        val pos = at.coerceIn(0, row.text.length)
        val newText = row.text.substring(0, pos) + text + row.text.substring(pos)
        val shifted = MarkRemap.remap(row.text, newText, row.marks)
        val withLink = MarkOps.addMark(shifted, pos, pos + text.length, InlineKind.Link, url)
        replaceRow(idx, row.copy(text = newText, marks = withLink))
        bump(row.id)
        desiredSelection = row.id to (pos + text.length)
        notifyEdit()
    }

    fun marksFor(rowId: String): List<com.exponential.app.ui.markdown.model.InlineMark> =
        (rows.firstOrNull { it.id == rowId } as? EditorRow.Para)?.marks ?: emptyList()

    fun attrsFor(rowId: String): ParagraphAttrs? =
        (rows.firstOrNull { it.id == rowId } as? EditorRow.Para)?.attrs

    // -- Paragraph kind ---------------------------------------------------------

    fun cycleHeading(rowId: String) = mutateAttrs(rowId) { a ->
        val next = when {
            a.kind != BlockKind.Heading -> 1
            a.headingLevel >= 3 -> 0
            else -> a.headingLevel + 1
        }
        if (next == 0) ParagraphAttrs.PLAIN else ParagraphAttrs(kind = BlockKind.Heading, headingLevel = next)
    }

    fun toggleList(rowId: String, type: ListType) = mutateAttrs(rowId) { a ->
        if (a.kind == BlockKind.ListItem && a.listType == type) ParagraphAttrs.PLAIN
        else ParagraphAttrs(kind = BlockKind.ListItem, listType = type)
    }.also { renumberOrdered() }

    fun toggleQuote(rowId: String) = mutateAttrs(rowId) { a ->
        if (a.kind == BlockKind.Blockquote) ParagraphAttrs.PLAIN
        else ParagraphAttrs(kind = BlockKind.Blockquote)
    }

    fun toggleCodeBlock(rowId: String) = mutateAttrs(rowId) { a ->
        if (a.kind == BlockKind.CodeBlock) ParagraphAttrs.PLAIN
        else ParagraphAttrs(kind = BlockKind.CodeBlock)
    }

    fun toggleChecklistChecked(rowId: String) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.Para ?: return
        if (row.attrs.kind != BlockKind.ListItem || row.attrs.listType != ListType.Checklist) return
        replaceRow(idx, row.copy(attrs = row.attrs.copy(checked = !row.attrs.checked)))
        setFocused(rowId)
        notifyEdit()
    }

    private fun mutateAttrs(rowId: String, transform: (ParagraphAttrs) -> ParagraphAttrs) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.Para ?: return
        replaceRow(idx, row.copy(attrs = transform(row.attrs)))
        bump(row.id) // re-seed the field so glyph / indent / text size update cleanly
        desiredSelection = row.id to ((selection?.takeIf { it.first == row.id }?.second?.first) ?: row.text.length)
        notifyEdit()
    }

    // -- Images -----------------------------------------------------------------

    fun insertImage(image: PendingImage): String =
        doInsertImage(url = draftUrl(), alt = "image", pending = image)

    /**
     * Insert an image with a caller-supplied URL (real `/api/attachments/...` or a
     * `draft://` placeholder), preserving the existing `onUploadImage` contract.
     * A non-null [pending] supplies in-memory bytes so the tile previews before /
     * during the host's own upload. Returns the inserted row's id (for
     * [runUpload] / [retryUpload]).
     */
    fun insertImageUrl(url: String, alt: String = "image", pending: PendingImage? = null): String =
        doInsertImage(url = url, alt = alt, pending = pending)

    private fun doInsertImage(url: String, alt: String, pending: PendingImage?): String {
        if (pending != null) pendingImages[url] = pending
        val imageRow = EditorRow.Image(url = url, alt = alt)

        val targetId = focusedRowId ?: selection?.first
        val targetIdx = rows.indexOfFirst { it.id == targetId }
        val targetRow = rows.getOrNull(targetIdx) as? EditorRow.Para

        if (targetRow == null) {
            val after = EditorRow.Para(text = "", attrs = ParagraphAttrs.PLAIN, marks = emptyList())
            val next = rows.toMutableList()
            next.add(imageRow)
            next.add(after)
            rows = EditorRows.normalize(next)
            uploadStates[imageRow.id] = ImageUploadState.Idle
            bumpAll()
            focusedRowId = after.id
            desiredSelection = after.id to 0
            notifyEdit()
            return imageRow.id
        }

        val caret = (selection?.takeIf { it.first == targetRow.id }?.second?.first ?: targetRow.text.length)
            .coerceIn(0, targetRow.text.length)
        val before = targetRow.copy(
            text = targetRow.text.substring(0, caret),
            marks = MarkOps.slice(targetRow.marks, 0, caret),
        )
        val after = EditorRow.Para(
            text = targetRow.text.substring(caret),
            attrs = ParagraphAttrs.PLAIN,
            marks = MarkOps.slice(targetRow.marks, caret, targetRow.text.length),
        )
        val next = rows.toMutableList()
        next[targetIdx] = before
        next.add(targetIdx + 1, imageRow)
        next.add(targetIdx + 2, after)
        rows = EditorRows.normalize(next)
        uploadStates[imageRow.id] = ImageUploadState.Idle
        bumpAll()
        focusedRowId = after.id
        desiredSelection = after.id to 0
        notifyEdit()
        return imageRow.id
    }

    // Host-supplied upload per image row, kept around while the upload can
    // still fail so the Failed overlay's Retry can re-invoke it.
    private val uploaders = mutableMapOf<String, suspend () -> String?>()

    /**
     * Run the host [upload] for an inserted image row: marks it Uploading, swaps
     * the row's URL (and preview-bytes key) to the returned URL on success, and
     * marks it Failed on error — the failed row keeps its bytes + uploader so
     * [retryUpload] can try again. Mirrors the iOS editor's per-block upload
     * lifecycle.
     */
    suspend fun runUpload(rowId: String, upload: suspend () -> String?) {
        if (rows.none { it.id == rowId }) return
        uploaders[rowId] = upload
        uploadStates[rowId] = ImageUploadState.Uploading
        uploadErrors.remove(rowId)
        val newUrl = runCatching { upload() }
            .onFailure { error -> uploadErrors[rowId] = error.message ?: "Upload failed" }
            .getOrNull()
        val current = rows.firstOrNull { it.id == rowId } as? EditorRow.Image
        if (current == null) {
            // Row was deleted while the upload ran — drop all tracking.
            uploaders.remove(rowId)
            uploadStates.remove(rowId)
            uploadErrors.remove(rowId)
            return
        }
        if (newUrl == null) {
            uploadStates[rowId] = ImageUploadState.Failed
            return
        }
        if (newUrl != current.url) {
            // Keep the local preview bytes under the new key so the tile keeps
            // rendering without waiting on a network fetch.
            pendingImages.remove(current.url)?.let { pendingImages[newUrl] = it }
            setImageUrl(rowId, newUrl)
        }
        uploaders.remove(rowId)
        uploadStates[rowId] = ImageUploadState.Idle
        uploadErrors.remove(rowId)
        notifyEdit()
    }

    /** Re-run a failed upload (no-op unless the row still has a registered uploader). */
    suspend fun retryUpload(rowId: String) {
        val upload = uploaders[rowId] ?: return
        runUpload(rowId, upload)
    }

    fun deleteImageRow(rowId: String) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val img = rows[idx] as? EditorRow.Image ?: return
        dropPendingDraft(img)
        uploadStates.remove(img.id)
        uploadErrors.remove(img.id)
        uploaders.remove(img.id)

        val prevIdx = idx - 1
        val nextIdx = idx + 1
        val next = rows.toMutableList()
        val prev = next.getOrNull(prevIdx)
        val after = next.getOrNull(nextIdx)
        if (prev is EditorRow.Para && after is EditorRow.Para) {
            val mergePoint = prev.text.length
            val merged = prev.copy(
                text = prev.text + after.text,
                marks = prev.marks + MarkOps.offset(after.marks, mergePoint),
            )
            next[prevIdx] = merged
            next.removeAt(nextIdx)
            next.removeAt(idx)
            rows = EditorRows.normalize(next)
            bumpAll()
            focusedRowId = merged.id
            desiredSelection = merged.id to mergePoint
        } else {
            next.removeAt(idx)
            rows = EditorRows.normalize(next)
            bumpAll()
        }
        notifyEdit()
    }

    private fun dropPendingDraft(img: EditorRow.Image) {
        if (isDraftUrl(img.url)) pendingImages.remove(img.url)
    }

    /**
     * Upload all draft images concurrently and swap their URLs to the returned
     * attachment paths. Returns true only if every draft resolved (all-or-nothing,
     * matching iOS). On failure, failed drafts keep their bytes for retry.
     */
    suspend fun commitPendingImages(uploader: suspend (PendingImage) -> String?): Boolean {
        removeDanglingDrafts()
        val drafts = rows.filterIsInstance<EditorRow.Image>()
            .filter { isDraftUrl(it.url) && pendingImages[it.url] != null }
        if (drafts.isEmpty()) return !hasUncommittedDrafts

        drafts.forEach { uploadStates[it.id] = ImageUploadState.Uploading }

        val results = coroutineScope {
            drafts.map { d ->
                async { d to runCatching { uploader(pendingImages[d.url]!!) }.getOrNull() }
            }.awaitAll()
        }

        var allOk = true
        for ((row, realUrl) in results) {
            if (realUrl != null) {
                setImageUrl(row.id, realUrl)
                pendingImages.remove(row.url)
                uploadStates[row.id] = ImageUploadState.Idle
            } else {
                uploadStates[row.id] = ImageUploadState.Failed
                allOk = false
            }
        }
        return allOk && !hasUncommittedDrafts
    }

    private fun setImageUrl(rowId: String, url: String) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val img = rows[idx] as? EditorRow.Image ?: return
        replaceRow(idx, img.copy(url = url))
    }

    private fun removeDanglingDrafts() {
        val next = rows.toMutableList()
        var changed = false
        for (i in next.indices.reversed()) {
            val r = next[i]
            if (r is EditorRow.Image && isDraftUrl(r.url) && pendingImages[r.url] == null) {
                uploadStates.remove(r.id)
                uploaders.remove(r.id)
                next.removeAt(i)
                changed = true
            }
        }
        if (changed) {
            rows = EditorRows.normalize(next)
            bumpAll()
        }
    }

    // -- Internals --------------------------------------------------------------

    private fun replaceRow(idx: Int, row: EditorRow) {
        rows = rows.toMutableList().also { it[idx] = row }
    }

    /** Renumber consecutive ordered list items (1,2,3…) so serialization matches the UI. */
    private fun renumberOrdered() {
        val next = rows.toMutableList()
        var counter = 0
        var prevWasOrdered = false
        var changed = false
        for (i in next.indices) {
            val r = next[i] as? EditorRow.Para
            val isOrdered = r != null && r.attrs.kind == BlockKind.ListItem && r.attrs.listType == ListType.Ordered
            if (isOrdered) {
                counter = if (prevWasOrdered) counter + 1 else 1
                if (r!!.attrs.orderedIndex != counter) {
                    next[i] = r.copy(attrs = r.attrs.copy(orderedIndex = counter))
                    changed = true
                }
            } else {
                counter = 0
            }
            prevWasOrdered = isOrdered
        }
        if (changed) rows = next
    }

    private fun bump(id: String) {
        revCounter++
        revisions[id] = revCounter
    }

    private fun bumpAll() {
        for (r in rows) {
            revCounter++
            revisions[r.id] = revCounter
        }
    }

    private fun notifyEdit() {
        onEdit?.invoke()
    }
}
