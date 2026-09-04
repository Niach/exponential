import {
  Table,
  TableCell,
  TableHeader,
  TableRow,
} from "@tiptap/extension-table"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"
import { Plugin, PluginKey, TextSelection } from "@tiptap/pm/state"
import { addRowAfter, isInTable, selectedRect } from "@tiptap/pm/tables"
import type { Editor } from "@tiptap/core"
import type { MarkdownSerializerState } from "prosemirror-markdown"
import type { MarkdownNodeSpec } from "tiptap-markdown"

// EXP-726 — GFM pipe tables in the markdown interchange.
//
// The canonical wire form every client emits (byte-locked by the shared
// CONTRACT_FIXTURES across web/desktop/iOS/Android):
//
//   | a | b |
//   | --- | --- |
//   | 1 | 2 |
//
// One space each side of the cell text, no column-width padding, rows joined
// by `\n`, and a blank line before/after (that falls out of closeBlock).
// Delimiter cells carry the alignment (`---` / `:---` / `:---:` / `---:`),
// an empty cell is `|  |`, `|` inside a cell is written `\|`, and a newline
// inside a cell becomes a space. Row 0 is ALWAYS the header row — GFM has no
// other shape — so a header-only table (`| a |\n| --- |`) is valid and a body
// row can never become row 0.
//
// tiptap-markdown ships its own default `table` serializer, but it pads
// nothing, ignores alignment, drops cell content that trims to empty and
// escapes no pipes. getMarkdownSpec (tiptap-markdown/src/util/extensions.js)
// merges specs by node NAME with the extension's own spec winning, so the
// override below replaces it wholesale.

/** GFM cell text: a newline collapses to a space, `|` is escaped. */
export function escapeTableCellText(text: string): string {
  return text.replace(/\n/g, ` `).replace(/\|/g, `\\|`)
}

/**
 * An in-cell image lives in the document as the LITERAL text `![alt](src)`
 * (see the parse hook on MarkdownTable). prosemirror-markdown's `esc()` then
 * escapes its brackets on the way out, and `!\[alt\](src)` is neither what
 * the other clients emit nor a match for `markdownImagePattern`
 * (lib/storage/issue-attachments.ts) — the attachment would go unreferenced.
 * Undo that one escape, leaving every other escape in the cell alone.
 *
 * Deliberately conservative: the alt run stops at the first escaped `]` and
 * the destination must be a bare URL, so an alt that itself contains a
 * bracket stays escaped rather than risk turning cell text that merely looks
 * like `!\[…\]` + a later `\](…)` into a link on the next parse.
 */
export function restoreTableCellImages(text: string): string {
  return text.replace(
    /!\\\[((?:\\[^\]]|[^\\\]])*)\\\]\(([^)\s]*)\)/g,
    `![$1]($2)`
  )
}

/** The delimiter-row cell for a column's alignment attribute. */
export function tableDelimiterCell(align: unknown): string {
  if (align === `left`) return `:---`
  if (align === `center`) return `:---:`
  if (align === `right`) return `---:`
  return `---`
}

// `out` and `nodes` are real, load-bearing members of prosemirror-markdown's
// serializer state but are absent from its published .d.ts.
type SerializerNode = (
  state: MarkdownSerializerState,
  node: ProseMirrorNode,
  parent: ProseMirrorNode,
  index: number
) => void
type SerializerInternals = MarkdownSerializerState & {
  out: string
  nodes: Record<string, SerializerNode | undefined>
}

function writeCell(
  state: MarkdownSerializerState,
  cell: ProseMirrorNode | null
) {
  const internals = state as SerializerInternals
  state.write(` `)
  const start = internals.out.length
  // Cells hold exactly ONE paragraph (see MarkdownTableCell below), rendered
  // as inline content. `fromBlockStart: false` keeps prosemirror-markdown
  // from escaping start-of-line constructs mid-row.
  const paragraph = cell?.firstChild
  if (paragraph) state.renderInline(paragraph, false)
  // Trimmed AFTER the newline collapse: a hard break at the cell's start or
  // end would otherwise leave a second space next to the pipe padding this
  // writer adds, so the row only converged on the canonical `| a |` shape
  // after an extra round trip.
  const text = restoreTableCellImages(
    escapeTableCellText(internals.out.slice(start)).trim()
  )
  internals.out = internals.out.slice(0, start) + text
  state.write(` |`)
}

function serializeTable(state: MarkdownSerializerState, node: ProseMirrorNode) {
  const internals = state as SerializerInternals
  const rows: ProseMirrorNode[] = []
  node.forEach((row) => rows.push(row))
  if (rows.length === 0) return

  const header = rows[0]
  const columns = header.childCount

  // A hard break has no GFM representation inside a cell (`\` + newline would
  // end the row); the contract says it becomes a space.
  const previousHardBreak = internals.nodes.hardBreak
  internals.nodes.hardBreak = (breakState) => {
    breakState.write(` `)
  }

  try {
    // Every line is written WITHOUT a trailing newline — the block separator
    // is flushClose's job (a trailing one would leak into the document's
    // final bytes).
    rows.forEach((row, rowIndex) => {
      if (rowIndex > 0) state.ensureNewLine()
      state.write(`|`)
      for (let column = 0; column < columns; column++) {
        // maybeChild, not child: a hand-authored ragged row must not
        // throw and take the whole document's serialization with it.
        writeCell(state, row.maybeChild(column))
      }
      if (rowIndex === 0) {
        state.ensureNewLine()
        state.write(`|`)
        for (let column = 0; column < columns; column++) {
          state.write(
            ` ${tableDelimiterCell(header.child(column).attrs.align)} |`
          )
        }
      }
    })
  } finally {
    internals.nodes.hardBreak = previousHardBreak
  }

  state.closeBlock(node)
}

/**
 * One inline paragraph per cell — the GFM cell model. Restricting the content
 * expression (the default is `block+`) also makes Enter/splitBlock and every
 * block input rule a no-op inside a cell, so nothing unserializable can be
 * typed into one.
 */
export const MarkdownTableCell = TableCell.extend({
  content: `paragraph`,
})

export const MarkdownTableHeader = TableHeader.extend({
  content: `paragraph`,
})

const trailingParagraphPluginKey = new PluginKey(
  `markdownTableTrailingParagraph`
)

/** StarterKit's `trailingNode` config — see trailingParagraphPlugin below. */
export const tableTrailingNodeOptions = { notAfter: [`table`] }

/**
 * A table as the document's LAST block leaves nowhere to put the caret —
 * there is no way to type after it. Keep an empty paragraph behind it.
 * MarkdownParagraph drops trailing empty paragraphs on serialize, so the
 * stored bytes are unaffected.
 *
 * A read-only editor has no caret to rescue, and markdown-editor.tsx
 * dispatches an empty transaction on mount, so an ungated plugin would append
 * a visible blank row under every read-only body ending in a table. The
 * editable check must happen per transaction, not once at construction —
 * `setEditable` flips it after the editor exists.
 *
 * StarterKit's own TrailingNode would append the same paragraph, ungated, so
 * it has to stand down for tables: pass `tableTrailingNodeOptions` as its
 * `trailingNode` config wherever these extensions are registered.
 */
export function trailingParagraphPlugin(editor: Editor): Plugin {
  return new Plugin({
    key: trailingParagraphPluginKey,
    appendTransaction: (_transactions, _oldState, newState) => {
      if (!editor.isEditable) return null
      const last = newState.doc.lastChild
      if (!last || last.type.name !== `table`) return null
      const paragraph = newState.schema.nodes.paragraph
      if (!paragraph) return null
      return newState.tr.insert(newState.doc.content.size, paragraph.create())
    },
  })
}

/**
 * Move the selection to just after the table the caret sits in, if any, and
 * report whether it moved.
 *
 * A cell holds exactly ONE paragraph, so inserting a BLOCK node (an uploaded
 * image) at a caret inside a cell splits the table in half. Callers park the
 * selection below the table first, and the image lands there instead.
 */
export function moveSelectionAfterTable(editor: Editor): boolean {
  const { state } = editor
  if (!isInTable(state)) return false
  const { $from } = state.selection
  for (let depth = $from.depth; depth > 0; depth--) {
    if ($from.node(depth).type.name !== `table`) continue
    const after = $from.after(depth)
    editor.view.dispatch(
      state.tr.setSelection(TextSelection.near(state.doc.resolve(after)))
    )
    return true
  }
  return false
}

/**
 * Enter inside a table moves to the same column of the next row (and grows
 * the table by one row when already on the last one) instead of splitting the
 * cell's paragraph.
 */
export function enterInTable(editor: Editor): boolean {
  const { state } = editor
  if (!isInTable(state)) return false
  const from = selectedRect(state)
  const column = from.left
  const row = from.top

  if (row + 1 >= from.map.height) {
    addRowAfter(editor.state, (tr) => editor.view.dispatch(tr))
  }

  const rect = selectedRect(editor.state)
  const target = row + 1
  if (target >= rect.map.height) return true
  const cellPos =
    rect.tableStart + rect.map.map[target * rect.map.width + column]
  const doc = editor.state.doc
  editor.view.dispatch(
    editor.state.tr
      .setSelection(TextSelection.near(doc.resolve(cellPos + 1)))
      .scrollIntoView()
  )
  return true
}

export const MarkdownTable = Table.extend({
  addStorage() {
    return {
      ...this.parent?.(),
      markdown: {
        serialize: serializeTable,
        parse: {
          updateDOM(element) {
            // A cell holds one inline paragraph, so a block image inside one
            // has no home in the schema and ProseMirror would silently drop
            // it. The cross-client contract keeps such an image as its
            // LITERAL markdown text instead of losing the alt/src.
            element.querySelectorAll(`td img, th img`).forEach((image) => {
              const alt = image.getAttribute(`alt`) ?? ``
              const src = image.getAttribute(`src`) ?? ``
              image.replaceWith(
                element.ownerDocument.createTextNode(`![${alt}](${src})`)
              )
            })
          },
        },
      } satisfies MarkdownNodeSpec,
    }
  },

  addCommands() {
    return {
      ...this.parent?.(),
      // A merged (or split) cell has no GFM form: the wire contract is a
      // RECTANGULAR grid of cells holding exactly one inline paragraph each
      // (CONTRACT_FIXTURES, mirrored on desktop/iOS/Android). A merge also
      // moves the swallowed cell's paragraph into the survivor, which the
      // serializer above would then hoist out of the table entirely. No UI
      // offers these, but `editor.commands.mergeCells()` stayed reachable —
      // make the whole family a no-op instead.
      mergeCells: () => () => false,
      splitCell: () => () => false,
      mergeOrSplit: () => () => false,
    }
  },

  addKeyboardShortcuts() {
    return {
      // Declared AFTER StarterKit in the editor's extension list, so this
      // keymap outranks splitBlock (tiptap builds plugins from the reversed
      // list) while the autocomplete extension, declared later still, keeps
      // Enter/Tab for its candidate menu.
      ...this.parent?.(),
      Enter: () => enterInTable(this.editor),
      // HardBreak has no GFM form inside a cell — swallow both gestures
      // rather than insert a node the serializer has to flatten away.
      "Shift-Enter": () => isInTable(this.editor.state),
      "Mod-Enter": () => isInTable(this.editor.state),
    }
  },

  addProseMirrorPlugins() {
    return [...(this.parent?.() ?? []), trailingParagraphPlugin(this.editor)]
  },
}).configure({
  // Column widths are not representable in GFM — a resized column would be a
  // silent local-only edit.
  resizable: false,
  HTMLAttributes: { class: `editor-table` },
})

export const MarkdownTableExtensions = [
  MarkdownTable,
  TableRow,
  MarkdownTableHeader,
  MarkdownTableCell,
]
