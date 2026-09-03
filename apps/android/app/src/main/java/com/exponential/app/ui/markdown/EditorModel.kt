package com.exponential.app.ui.markdown

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.PendingImage
import com.exponential.app.ui.markdown.model.TableCell
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope

/**
 * Single source of truth for the block markdown editor — the Compose analog of
 * iOS `IssueEditorModel`. Owns the flat [EditorRow] document (multi-line
 * [EditorRow.TextRun]s split only by images, EXP-534) and routes every edit
 * through intent methods; markdown is derived only at save ([currentMarkdown]),
 * never per keystroke.
 *
 * Revisions are bumped ONLY when the MODEL diverges from what the field
 * reported (load, list-exit rewrite, toolbar insert, image insert/delete,
 * attribute toggles) so a field never clobbers the characters the user just
 * typed — mirroring the iOS revision discipline. An ordinary Enter or
 * multi-line paste needs NO reseed: the field already holds the new text, and
 * [updateRun] only remaps marks + paragraph attrs onto it.
 */
@Stable
class EditorModel {

    enum class ImageUploadState { Idle, Uploading, Failed }

    var rows by mutableStateOf<List<EditorRow>>(listOf(EditorRows.emptyRun()))
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

    /**
     * Row whose `@`/`#` autocomplete an explicit toolbar insert just armed
     * (EXP-322). The field only opens its menu on a TEXT change, and a toolbar
     * insert reaches it through the revision re-seed, which is
     * indistinguishable from a reseeding rewrite — hence this explicit
     * one-shot signal. Set only by [insertPlainText].
     */
    var autocompleteArmRowId by mutableStateOf<String?>(null)
        private set

    /** Consumes the arm signal for [id]; true when this row was the target. */
    fun consumeAutocompleteArm(id: String): Boolean {
        if (autocompleteArmRowId != id) return false
        autocompleteArmRowId = null
        return true
    }

    // -- Text editing -----------------------------------------------------------

    /**
     * Apply the field's post-edit text for a run — the ONE entry point for
     * every IME edit: typing, Enter (a plain '\n' now that a run is one
     * multi-line field), backspace over a newline, and multi-line paste. One
     * [TextDiff] (caret-pinned to disambiguate repeated surroundings) remaps
     * both the inline marks and the paragraph-attribute list; no revision bump,
     * so the field's own keystrokes are never clobbered.
     *
     * The one rewrite: Enter on an EMPTY list item exits the list instead of
     * continuing it — the model drops the '\n', demotes the line to a plain
     * paragraph, and reseeds the field (revision bump + desired caret).
     */
    fun updateRun(rowId: String, newText: String, caret: Int) {
        val idx = rows.indexOfFirst { it.id == rowId }
        // Table cells are fields too (EXP-726) and reach this same entry point
        // keyed by their CELL id, which is never a row id.
        if (idx < 0) {
            updateCell(rowId, newText, caret)
            return
        }
        val row = rows[idx] as? EditorRow.TextRun ?: return
        if (row.text == newText) return
        val diff = TextDiff.of(row.text, newText, caret)

        val exitIndex = emptyListItemEnterIndex(row, newText, diff)
        if (exitIndex != null) {
            val paras = row.paragraphs.toMutableList()
            paras[exitIndex] = ParagraphAttrs.PLAIN
            replaceRow(idx, row.copy(paragraphs = paras))
            bump(row.id)
            desiredSelection = row.id to diff.removedStart
            renumberOrdered()
            notifyEdit()
            return
        }

        val remapped = MarkRemap.remap(diff, newText, row.marks)
        val finalMarks = applyPendingMarks(rowId, row.text, newText, caret, remapped)
        val newParas = ParaRemap.remap(diff, row.text, newText, row.paragraphs)
        val structural = newParas.size != row.paragraphs.size
        rows = rows.toMutableList().also {
            it[idx] = row.copy(text = newText, paragraphs = newParas, marks = finalMarks)
        }
        selection = rowId to (caret..caret)
        if (structural) renumberOrdered()
        notifyEdit()
    }

    /**
     * The list-exit rule: the edit is a lone '\n' typed on an empty ListItem
     * line. Returns that line's paragraph index, or null.
     */
    private fun emptyListItemEnterIndex(
        row: EditorRow.TextRun,
        newText: String,
        diff: TextDiff,
    ): Int? {
        if (!diff.isPureInsertion || diff.insertedLen != 1) return null
        if (newText.getOrNull(diff.removedStart) != '\n') return null
        val index = ParaRemap.paraIndexAt(row.text, diff.removedStart)
        val (start, end) = ParaRemap.paragraphBounds(row.text, index)
        if (start != end || diff.removedStart != start) return null
        val attrs = row.paragraphs.getOrNull(index) ?: return null
        return if (attrs.kind == BlockKind.ListItem) index else null
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

    /**
     * Backspace on a collapsed caret at a formatted line's start: clear that
     * line's block formatting instead of deleting into the previous line
     * (first-press-demotes, matching the old per-row editor and iOS).
     */
    fun clearParagraphFormat(rowId: String, caret: Int) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.TextRun ?: return
        val index = ParaRemap.paraIndexAt(row.text, caret)
        val attrs = row.paragraphs.getOrNull(index) ?: return
        if (attrs.kind == BlockKind.Paragraph) return
        val paras = row.paragraphs.toMutableList()
        paras[index] = ParagraphAttrs.PLAIN
        replaceRow(idx, row.copy(paragraphs = paras))
        bump(row.id)
        desiredSelection = row.id to caret.coerceIn(0, row.text.length)
        renumberOrdered()
        notifyEdit()
    }

    /**
     * Backspace at offset 0 of a run whose previous row is an image: delete the
     * image and merge this run into the run above it (the two runs' adjacent
     * lines join into one, keeping the upper line's attrs).
     */
    fun backspaceAtRunStart(rowId: String) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx <= 0) return
        val row = rows[idx] as? EditorRow.TextRun ?: return
        val prev = rows[idx - 1] as? EditorRow.Image ?: return

        dropPendingDraft(prev)
        uploadStates.remove(prev.id)
        uploadErrors.remove(prev.id)
        uploaders.remove(prev.id)
        val prevPrevIdx = idx - 2
        val next = rows.toMutableList()
        val target = next.getOrNull(prevPrevIdx) as? EditorRow.TextRun
        if (target != null) {
            val mergePoint = target.text.length
            val merged = joinRuns(target, row)
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

    /**
     * Join two runs that an image no longer separates: [first]'s last line and
     * [second]'s first line become ONE line (keeping [first]'s attrs for it),
     * exactly like the old per-row image-delete merge.
     */
    private fun joinRuns(first: EditorRow.TextRun, second: EditorRow.TextRun): EditorRow.TextRun =
        first.copy(
            text = first.text + second.text,
            paragraphs = first.paragraphs + second.paragraphs.drop(1),
            marks = first.marks + MarkOps.offset(second.marks, first.text.length),
        )

    // -- Table cells (EXP-726) --------------------------------------------------

    /** Where a table cell lives: its table ROW's id plus (row, col), header = row 0. */
    data class CellLocation(val tableRowId: String, val row: Int, val col: Int)

    /** The location of the cell with [cellId], or null when it is not a cell id. */
    fun locateCell(cellId: String): CellLocation? {
        for (r in rows) {
            if (r !is EditorRow.Table) continue
            val at = r.table.locate(cellId) ?: continue
            return CellLocation(r.id, at.first, at.second)
        }
        return null
    }

    /** The cell with [cellId], if any — the table field seeds its value from it. */
    fun cell(cellId: String): TableCell? {
        val loc = locateCell(cellId) ?: return null
        val table = (rows.firstOrNull { it.id == loc.tableRowId } as? EditorRow.Table)?.table
        return table?.cellAt(loc.row, loc.col)
    }

    /**
     * A cell field's post-edit text. A cell is ONE inline paragraph, so any
     * newline the IME or a paste brought in folds to a space before the mark
     * remap; no revision bump, exactly like [updateRun].
     */
    private fun updateCell(cellId: String, newText: String, caret: Int) {
        val loc = locateCell(cellId) ?: return
        val idx = rows.indexOfFirst { it.id == loc.tableRowId }
        val tableRow = rows.getOrNull(idx) as? EditorRow.Table ?: return
        val cell = tableRow.table.cellAt(loc.row, loc.col) ?: return
        val text = newText.replace('\n', ' ')
        if (cell.text == text) return
        val safeCaret = caret.coerceIn(0, text.length)
        val diff = TextDiff.of(cell.text, text, safeCaret)
        val marks = MarkRemap.remap(diff, text, cell.marks)
        replaceRow(
            idx,
            tableRow.copy(table = tableRow.table.withCell(loc.row, loc.col, cell.copy(text = text, marks = marks))),
        )
        selection = cellId to (safeCaret..safeCaret)
        notifyEdit()
    }

    /**
     * Set a table cell's text by coordinate (header = row 0) — the host-facing
     * form of [updateCell] for callers that address the grid, not the cell id.
     */
    fun updateTableCell(rowId: String, row: Int, col: Int, text: String, caret: Int = text.length) {
        val tableRow = rows.firstOrNull { it.id == rowId } as? EditorRow.Table ?: return
        val cell = tableRow.table.cellAt(row, col) ?: return
        updateCell(cell.id, text, caret)
    }

    // -- Inline marks -----------------------------------------------------------

    fun toggleMark(rowId: String, range: IntRange, kind: InlineKind, href: String? = null) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.TextRun ?: return
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

    /**
     * Strip every inline mark over [range] and drop any queued pending marks —
     * the rail's "Clear formatting" (web parity: `unsetAllMarks`). A COLLAPSED
     * range has no inline run to strip, so it clears the touched paragraph's
     * block formatting instead (web's `clearNodes`), which is the only thing a
     * caret-only tap can meaningfully undo.
     */
    fun clearFormatting(rowId: String, range: IntRange) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.TextRun ?: return
        val start = range.first.coerceIn(0, row.text.length)
        val end = range.last.coerceIn(start, row.text.length)
        pendingMarks = emptySet()
        pendingAnchor = null
        if (end > start) {
            var marks = row.marks
            for (kind in InlineKind.entries) marks = MarkOps.removeMark(marks, start, end, kind)
            replaceRow(idx, row.copy(marks = marks))
        } else {
            val index = ParaRemap.paraIndexAt(row.text, start)
            val paras = row.paragraphs.toMutableList()
            if (index !in paras.indices) return
            paras[index] = ParagraphAttrs.PLAIN
            replaceRow(idx, row.copy(paragraphs = paras))
            // Only the paragraph path reseeds: attrs drive indent / text size,
            // and the text itself is untouched so a live selection survives.
            bump(row.id)
            desiredSelection = row.id to start
            renumberOrdered()
        }
        // Re-assert focus for the same reason [toggleMark] does — the rail tap
        // took OS focus off the field.
        setFocused(rowId)
        notifyEdit()
    }

    /**
     * Insert plain text at the caret of the active row (falls back to the end of
     * the last text run) — the comment composer's `@` affordance (EXP-240).
     * Mirrors [insertLinkText] sans the mark; re-asserts focus so the mention
     * autocomplete fires off the reseeded caret.
     */
    fun insertPlainText(text: String) {
        val rid = activeRowId
            ?: rows.lastOrNull { it is EditorRow.TextRun }?.id
            ?: return
        val idx = rows.indexOfFirst { it.id == rid }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.TextRun ?: return
        val pos = (selection?.takeIf { it.first == rid }?.second?.first ?: row.text.length)
            .coerceIn(0, row.text.length)
        val newText = row.text.substring(0, pos) + text + row.text.substring(pos)
        replaceRow(idx, row.copy(text = newText, marks = MarkRemap.remap(row.text, newText, row.marks)))
        bump(row.id)
        setFocused(row.id)
        desiredSelection = row.id to (pos + text.length)
        // Only the `@`/`#` affordances may open an autocomplete menu the user
        // did not type a trigger for (EXP-322).
        if (text == "@" || text == "#") autocompleteArmRowId = row.id
        notifyEdit()
    }

    /** Insert link display text at [at] and mark it (used when there is no selection). */
    fun insertLinkText(rowId: String, at: Int, text: String, url: String) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.TextRun ?: return
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
        (rows.firstOrNull { it.id == rowId } as? EditorRow.TextRun)?.marks ?: emptyList()

    /**
     * The href of a link covering the START of [range] in [rowId], if any — the
     * link rail prefills its URL field with it so editing an existing link
     * replaces rather than blanks it.
     */
    fun linkHrefAt(rowId: String, range: IntRange): String? {
        val row = rows.firstOrNull { it.id == rowId } as? EditorRow.TextRun ?: return null
        val at = range.first
        return row.marks
            .firstOrNull { it.kind == InlineKind.Link && it.start <= at && it.end > at }
            ?.href
    }

    /**
     * The full [start, end) range of the link covering the START of [range] in
     * [rowId], if any (EXP-572). A caret (or partial selection) inside an
     * existing link edits THAT link — the rail expands its pending range to
     * this before opening, mirroring web's `extendMarkRange('link')` — so the
     * new href replaces the old one instead of landing as fresh link text in
     * the middle of the original.
     */
    fun linkRangeAt(rowId: String, range: IntRange): IntRange? {
        val row = rows.firstOrNull { it.id == rowId } as? EditorRow.TextRun ?: return null
        val at = range.first
        val link = row.marks
            .firstOrNull { it.kind == InlineKind.Link && it.start <= at && it.end > at }
            ?: return null
        return link.start..link.end
    }

    /** The paragraph under the caret (start of the active selection) — toolbar tint. */
    fun attrsAtCaret(): ParagraphAttrs? {
        val (rid, range) = activeSelection() ?: return null
        val row = rows.firstOrNull { it.id == rid } as? EditorRow.TextRun ?: return null
        return row.paragraphs.getOrNull(ParaRemap.paraIndexAt(row.text, range.first))
    }

    // -- Paragraph kind ---------------------------------------------------------
    // Toggles act on every paragraph the active selection touches (a collapsed
    // caret = its one line) — the multi-paragraph upgrade EXP-534's one-field-
    // per-run restructure unlocks. All-or-nothing semantics: if every touched
    // paragraph already has the target kind, all clear to plain.

    /**
     * Set every touched paragraph to heading [level] (1..3), or to a plain
     * paragraph for level 0 — the rail's single-select Text/H1/H2/H3 group.
     * Re-tapping the level every touched paragraph already has clears it, so
     * the active button doubles as its own off switch (web/iOS parity).
     */
    fun setHeading(rowId: String, level: Int) = mutateParaRange(rowId) { attrs ->
        val target = level.coerceIn(0, 3)
        val alreadyThatLevel = target > 0 &&
            attrs.all { it.kind == BlockKind.Heading && it.headingLevel == target }
        val v = if (target == 0 || alreadyThatLevel) ParagraphAttrs.PLAIN
        else ParagraphAttrs(kind = BlockKind.Heading, headingLevel = target)
        attrs.map { v }
    }

    fun toggleList(rowId: String, type: ListType) = mutateParaRange(rowId) { attrs ->
        if (attrs.all { it.kind == BlockKind.ListItem && it.listType == type }) {
            attrs.map { ParagraphAttrs.PLAIN }
        } else {
            attrs.map { ParagraphAttrs(kind = BlockKind.ListItem, listType = type) }
        }
    }.also { renumberOrdered() }

    fun toggleQuote(rowId: String) = mutateParaRange(rowId) { attrs ->
        if (attrs.all { it.kind == BlockKind.Blockquote }) attrs.map { ParagraphAttrs.PLAIN }
        else attrs.map { ParagraphAttrs(kind = BlockKind.Blockquote) }
    }

    fun toggleCodeBlock(rowId: String) = mutateParaRange(rowId) { attrs ->
        if (attrs.all { it.kind == BlockKind.CodeBlock }) attrs.map { ParagraphAttrs.PLAIN }
        else attrs.map { ParagraphAttrs(kind = BlockKind.CodeBlock) }
    }

    fun toggleChecklistChecked(rowId: String, paraIndex: Int) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.TextRun ?: return
        val attrs = row.paragraphs.getOrNull(paraIndex) ?: return
        if (attrs.kind != BlockKind.ListItem || attrs.listType != ListType.Checklist) return
        val paras = row.paragraphs.toMutableList()
        paras[paraIndex] = attrs.copy(checked = !attrs.checked)
        replaceRow(idx, row.copy(paragraphs = paras))
        setFocused(rowId)
        notifyEdit()
    }

    private fun mutateParaRange(
        rowId: String,
        transform: (List<ParagraphAttrs>) -> List<ParagraphAttrs>,
    ) {
        val idx = rows.indexOfFirst { it.id == rowId }
        if (idx < 0) return
        val row = rows[idx] as? EditorRow.TextRun ?: return
        val sel = selection?.takeIf { it.first == rowId }?.second
        val start = (sel?.first ?: row.text.length).coerceIn(0, row.text.length)
        val end = (sel?.last ?: row.text.length).coerceIn(start, row.text.length)
        val firstPara = ParaRemap.paraIndexAt(row.text, start)
        val lastPara = ParaRemap.paraIndexAt(row.text, end)
            .coerceIn(firstPara, row.paragraphs.lastIndex.coerceAtLeast(0))
        val paras = row.paragraphs.toMutableList()
        if (paras.isEmpty()) return
        val slice = (firstPara..lastPara).map { paras[it] }
        val replaced = transform(slice)
        for (i in firstPara..lastPara) paras[i] = replaced[i - firstPara]
        replaceRow(idx, row.copy(paragraphs = paras))
        // Re-seed the field so indent / text size update cleanly. When only
        // attributes changed the field's text and selection start still match,
        // so a live selection survives the (no-op) reseed.
        bump(row.id)
        desiredSelection = row.id to start
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

    /**
     * Append an image at the END of the description, ignoring the caret (EXP-327).
     * An image picked through the *file* picker has no meaningful insertion point —
     * the user was attaching, not typing — so it lands after everything already
     * written instead of splitting whatever paragraph happened to hold focus.
     */
    fun appendImageUrl(url: String, alt: String = "image", pending: PendingImage? = null): String =
        doInsertImage(url = url, alt = alt, pending = pending, atEnd = true)

    private fun doInsertImage(
        url: String,
        alt: String,
        pending: PendingImage?,
        atEnd: Boolean = false,
    ): String {
        if (pending != null) pendingImages[url] = pending
        val imageRow = EditorRow.Image(url = url, alt = alt)

        val targetId = if (atEnd) null else focusedRowId ?: selection?.first
        val targetIdx = rows.indexOfFirst { it.id == targetId }
        val targetRow = rows.getOrNull(targetIdx) as? EditorRow.TextRun

        if (targetRow == null) {
            val after = EditorRows.emptyRun()
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
        val caretPara = ParaRemap.paraIndexAt(targetRow.text, caret)
        val before = targetRow.copy(
            text = targetRow.text.substring(0, caret),
            paragraphs = targetRow.paragraphs.subList(0, caretPara + 1).toList(),
            marks = MarkOps.slice(targetRow.marks, 0, caret),
        )
        // The remainder's first line becomes a plain paragraph (the image cut
        // it loose from its old block context); later lines keep their attrs.
        val after = EditorRow.TextRun(
            text = targetRow.text.substring(caret),
            paragraphs = listOf(ParagraphAttrs.PLAIN) +
                targetRow.paragraphs.subList(caretPara + 1, targetRow.paragraphs.size),
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
            .onFailure { error -> uploadErrors[rowId] = trpcErrorMessage(error, "Upload failed") }
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
        if (prev is EditorRow.TextRun && after is EditorRow.TextRun) {
            val mergePoint = prev.text.length
            val merged = joinRuns(prev, after)
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
        renumberOrdered()
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
                async {
                    d to runCatching { uploader(pendingImages[d.url]!!) }
                        // Keep the reason so the Failed badge can explain WHY
                        // (e.g. the neutral storage-full message) instead of a
                        // bare retry state.
                        .onFailure { error -> uploadErrors[d.id] = trpcErrorMessage(error, "Upload failed") }
                        .getOrNull()
                }
            }.awaitAll()
        }

        var allOk = true
        for ((row, realUrl) in results) {
            if (realUrl != null) {
                setImageUrl(row.id, realUrl)
                pendingImages.remove(row.url)
                uploadStates[row.id] = ImageUploadState.Idle
                uploadErrors.remove(row.id)
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

    /**
     * Renumber consecutive ordered list items (1,2,3…) so serialization matches
     * the UI. Counters are PER DEPTH, mirroring the parser's ListFrame stack: a
     * nested list lives inside its parent item, so deeper paragraphs must not
     * break the parent's run and the parent's counter must never bleed into a
     * nested list (REV-31). A depth's counter resets when its run breaks — a
     * non-list paragraph, an image row, a shallower list paragraph (which
     * closes the nested list), or a different list type arriving at the same
     * depth (a marker-type switch starts a new list in CommonMark).
     */
    private fun renumberOrdered() {
        val counters = mutableListOf<Int>() // counters[d] = last ordered index emitted at depth d
        val types = mutableListOf<ListType>() // list type of the open run at depth d
        var changed = false
        val next = rows.map { r ->
            when (r) {
                // A block-level row breaks every open list run.
                is EditorRow.Image, is EditorRow.Table -> {
                    counters.clear()
                    types.clear()
                    r
                }
                is EditorRow.TextRun -> {
                    var rowChanged = false
                    val paras = r.paragraphs.toMutableList()
                    for (i in paras.indices) {
                        val a = paras[i]
                        if (a.kind != BlockKind.ListItem) {
                            counters.clear()
                            types.clear()
                            continue
                        }
                        val depth = a.listDepth.coerceAtLeast(0)
                        // This paragraph closes any deeper open lists…
                        while (counters.size > depth + 1) {
                            counters.removeAt(counters.size - 1)
                            types.removeAt(types.size - 1)
                        }
                        // …and opens its own level (plus any skipped intermediates).
                        val type = a.listType ?: ListType.Bullet // the serializer emits null as a bullet
                        while (counters.size <= depth) {
                            counters.add(0)
                            types.add(type)
                        }
                        if (types[depth] != type) {
                            counters[depth] = 0
                            types[depth] = type
                        }
                        if (type == ListType.Ordered) {
                            counters[depth] = counters[depth] + 1
                            if (a.orderedIndex != counters[depth]) {
                                paras[i] = a.copy(orderedIndex = counters[depth])
                                rowChanged = true
                            }
                        }
                    }
                    if (rowChanged) {
                        changed = true
                        r.copy(paragraphs = paras)
                    } else r
                }
            }
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
            // Cell fields re-seed off their own id's revision (EXP-726).
            if (r is EditorRow.Table) {
                for (c in r.table.allCells) {
                    revCounter++
                    revisions[c.id] = revCounter
                }
            }
        }
    }

    private fun notifyEdit() {
        emitted.addLast(currentMarkdown())
        while (emitted.size > EMITTED_HISTORY) emitted.removeFirst()
        onEdit?.invoke()
    }

    /**
     * Everything [onEdit] handed the host since the host last caught up —
     * the host echoes each value straight back as the editor's `markdown`
     * input, and an echo must never reload the editor (EXP-655).
     */
    private val emitted = ArrayDeque<String>()

    /**
     * Whether the host's `markdown` input should be loaded (true) or is an
     * echo of this model's own output (false) — [MarkdownEditor]'s rule for
     * its `LaunchedEffect(markdown)`.
     *
     * The effect body runs as a later main-looper message, and an IME
     * commit can land in between: the model has already moved on to `md2`
     * when the effect keyed on `md1` runs, so a plain `currentMarkdown() !=
     * markdown` reloaded the editor with its OWN previous output — every row
     * rebuilt with a fresh id, caret to the end, focus lost, blank lines
     * collapsed by the round-trip — the "jumps while typing" of EXP-655. Any
     * value this model emitted since the host last caught up is such an
     * echo; a newer one is already on its way.
     */
    fun reconcileHostMarkdown(markdown: String): Boolean {
        if (markdown == currentMarkdown()) {
            emitted.clear()
            return false
        }
        if (emitted.contains(markdown)) return false
        emitted.clear()
        return true
    }

    /**
     * Focus the very end of the document — a tap on the editor's empty band
     * below the last line (EXP-655). [EditorRows.normalize] guarantees the
     * last row is a text run (an empty one after a trailing image), so there
     * is always a line to land on and Enter always opens a new one.
     */
    fun focusEnd() {
        val last = rows.lastOrNull { it is EditorRow.TextRun } as? EditorRow.TextRun ?: return
        setFocused(last.id)
        desiredSelection = last.id to last.text.length
        // The field consumes desiredSelection on its revision effect.
        bump(last.id)
    }

    private companion object {
        const val EMITTED_HISTORY = 32
    }
}
