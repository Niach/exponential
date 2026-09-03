import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Markdown } from "tiptap-markdown"
import { MarkdownImage } from "@/lib/markdown-image"
import { MarkdownParagraph } from "@/components/issue-editor/markdown-paragraph"
import { TextSelection } from "@tiptap/pm/state"
import {
  moveTableColumn,
  moveTableRow,
  selectedRect,
  TableMap,
} from "@tiptap/pm/tables"
import { MarkdownTableExtensions } from "@/lib/markdown-table"
import { tableMenuModel } from "@/components/issue-editor/table-controls"

// EXP-726 — GFM tables travel through `issues.description` / `comments.body`
// and must come back BYTE-identical on every client. These fixtures are the
// shared cross-client corpus, mirrored in the desktop CONTRACT_FIXTURES, the
// Android MarkdownRoundTripTest and the iOS table suite.
function makeEditor(markdown: string) {
  return new Editor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] }, paragraph: false }),
      MarkdownParagraph,
      MarkdownImage,
      ...MarkdownTableExtensions,
      Markdown.configure({ html: false }),
    ],
    content: markdown,
  })
}

function markdownOf(editor: Editor): string {
  return (
    editor.storage as unknown as { markdown: { getMarkdown: () => string } }
  ).markdown.getMarkdown()
}

function roundTrip(markdown: string): string {
  const editor = makeEditor(markdown)
  const out = markdownOf(editor)
  editor.destroy()
  return out
}

const fixtures: Array<[string, string]> = [
  [`table_basic`, `| a | b |\n| --- | --- |\n| 1 | 2 |`],
  [
    `table_alignments`,
    `| l | c | r | n |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |`,
  ],
  [
    `table_inline_marks`,
    `| **bold** | [link](https://example.com) |\n| --- | --- |\n| \`code\` | *em* |`,
  ],
  [`table_escaped_pipe`, `| a \\| b | c |\n| --- | --- |\n| 1 | 2 |`],
  [`table_empty_cell`, `| a | b |\n| --- | --- |\n| 1 |  |`],
  [`table_header_only`, `| a | b |\n| --- | --- |`],
  [
    `table_between_paragraphs`,
    `before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter`,
  ],
  [
    `table_chip_cells`,
    `| @jane@example.com | #EXP-42 |\n| --- | --- |\n| x | y |`,
  ],
  [`table_unicode`, `| Grüße | 🚀 |\n| --- | --- |\n| ü | é |`],
]

describe(`markdown table round-trip`, () => {
  it.each(fixtures)(`%s survives byte-for-byte`, (_name, markdown) => {
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it.each(fixtures)(`%s is a fixpoint`, (_name, markdown) => {
    expect(roundTrip(roundTrip(markdown))).toBe(markdown)
  })

  // Kept OUT of the shared corpus (commonmark-java's behaviour is unverified)
  // but locked here: GFM unescapes `\|` before inline parsing, so a pipe
  // inside a code span round-trips through the same escape.
  it(`escapes a pipe inside a code span`, () => {
    const markdown = `| a |\n| --- |\n| \`x \\| y\` |`
    expect(roundTrip(markdown)).toBe(markdown)
  })

  // A cell holds ONE inline paragraph, so a block image inside one cannot
  // exist in the schema. Rather than let ProseMirror drop it, the parse hook
  // keeps its literal markdown text (the cross-client contract) — escaped on
  // the way back out, and a fixpoint from there.
  it(`keeps an image inside a cell as literal text`, () => {
    const once = roundTrip(
      `| a | b |\n| --- | --- |\n| ![alt](/api/attachments/x) | 2 |`
    )
    expect(once).toBe(
      `| a | b |\n| --- | --- |\n| !\\[alt\\](/api/attachments/x) | 2 |`
    )
    expect(roundTrip(once)).toBe(once)
  })

  it(`normalises a padded GitHub-style table to the canonical form`, () => {
    const padded = [
      `| Name    | Value |`,
      `| ------- | :---: |`,
      `| alpha   | 1     |`,
      `| beta    |       |`,
    ].join(`\n`)
    const canonical = `| Name | Value |\n| --- | :---: |\n| alpha | 1 |\n| beta |  |`
    expect(roundTrip(padded)).toBe(canonical)
    expect(roundTrip(canonical)).toBe(canonical)
  })

  it(`pads a ragged body row with empty cells`, () => {
    expect(roundTrip(`| a | b |\n| --- | --- |\n| 1 |`)).toBe(
      `| a | b |\n| --- | --- |\n| 1 |  |`
    )
  })

  it(`keeps a table between two paragraphs as its own block`, () => {
    const editor = makeEditor(
      `before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter`
    )
    const kinds: string[] = []
    editor.state.doc.forEach((node) => kinds.push(node.type.name))
    editor.destroy()
    expect(kinds).toEqual([`paragraph`, `table`, `paragraph`])
  })

  it(`parses the delimiter row into per-column align attrs`, () => {
    const editor = makeEditor(
      `| l | c | r | n |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |`
    )
    const table = editor.state.doc.child(0)
    const header = table.child(0)
    const aligns: Array<unknown> = []
    header.forEach((cell) => aligns.push(cell.attrs.align))
    const body = table.child(1)
    const bodyAligns: Array<unknown> = []
    body.forEach((cell) => bodyAligns.push(cell.attrs.align))
    editor.destroy()
    expect(aligns).toEqual([`left`, `center`, `right`, null])
    expect(bodyAligns).toEqual([`left`, `center`, `right`, null])
  })

  it(`makes every cell exactly one paragraph`, () => {
    const editor = makeEditor(`| a | b |\n| --- | --- |\n| 1 | 2 |`)
    const table = editor.state.doc.child(0)
    const header = table.child(0)
    expect(header.child(0).type.name).toBe(`tableHeader`)
    expect(header.child(0).childCount).toBe(1)
    expect(header.child(0).child(0).type.name).toBe(`paragraph`)
    expect(table.child(1).child(0).type.name).toBe(`tableCell`)
    editor.destroy()
  })
})

// ── keymap ──

function selectCell(editor: Editor, row: number, col: number) {
  const table = editor.state.doc.child(0)
  const map = TableMap.get(table)
  const cellPos = 1 + map.map[row * map.width + col]
  editor.view.dispatch(
    editor.state.tr.setSelection(
      TextSelection.near(editor.state.doc.resolve(cellPos + 1))
    )
  )
}

function pressKey(editor: Editor, init: KeyboardEventInit) {
  const event = new KeyboardEvent(`keydown`, { ...init, bubbles: true })
  return Boolean(
    editor.view.someProp(`handleKeyDown`, (handler) =>
      handler(editor.view, event)
    )
  )
}

function currentCell(editor: Editor) {
  const rect = selectedRect(editor.state)
  return { row: rect.top, col: rect.left }
}

describe(`markdown table keymap`, () => {
  it(`moves Enter to the same column of the next row`, () => {
    const editor = makeEditor(`| a | b |\n| --- | --- |\n| 1 | 2 |`)
    selectCell(editor, 0, 1)
    expect(pressKey(editor, { key: `Enter` })).toBe(true)
    expect(currentCell(editor)).toEqual({ row: 1, col: 1 })
    editor.destroy()
  })

  it(`grows the table when Enter lands on the last row`, () => {
    const editor = makeEditor(`| a | b |\n| --- | --- |\n| 1 | 2 |`)
    selectCell(editor, 1, 0)
    expect(pressKey(editor, { key: `Enter` })).toBe(true)
    expect(currentCell(editor)).toEqual({ row: 2, col: 0 })
    expect(markdownOf(editor)).toBe(
      `| a | b |\n| --- | --- |\n| 1 | 2 |\n|  |  |`
    )
    editor.destroy()
  })

  it(`swallows Shift-Enter and Mod-Enter inside a table`, () => {
    const editor = makeEditor(`| a | b |\n| --- | --- |\n| 1 | 2 |`)
    selectCell(editor, 1, 0)
    expect(pressKey(editor, { key: `Enter`, shiftKey: true })).toBe(true)
    // prosemirror-keymap binds `Mod-` to Meta on macOS and Ctrl elsewhere.
    expect(
      pressKey(editor, { key: `Enter`, metaKey: true }) ||
        pressKey(editor, { key: `Enter`, ctrlKey: true })
    ).toBe(true)
    expect(markdownOf(editor)).toBe(`| a | b |\n| --- | --- |\n| 1 | 2 |`)
    editor.destroy()
  })

  it(`leaves Enter alone outside a table`, () => {
    const editor = makeEditor(`before\n\n| a |\n| --- |\n| 1 |`)
    editor.commands.focus(`start`)
    expect(pressKey(editor, { key: `Enter` })).toBe(true)
    // StarterKit's splitBlock ran: the paragraph became two.
    expect(editor.state.doc.child(0).type.name).toBe(`paragraph`)
    expect(editor.state.doc.child(1).type.name).toBe(`paragraph`)
    editor.destroy()
  })

  it(`keeps an empty paragraph behind a trailing table`, () => {
    const editor = makeEditor(`| a |\n| --- |\n| 1 |`)
    selectCell(editor, 1, 0)
    const kinds: string[] = []
    editor.state.doc.forEach((node) => kinds.push(node.type.name))
    // ...and the trailing paragraph never reaches the wire.
    expect(kinds).toEqual([`table`, `paragraph`])
    expect(markdownOf(editor)).toBe(`| a |\n| --- |\n| 1 |`)
    editor.destroy()
  })
})

// ── row/column moves ──

describe(`markdown table moves`, () => {
  it(`moves a body row down`, () => {
    const editor = makeEditor(`| a |\n| --- |\n| 1 |\n| 2 |`)
    selectCell(editor, 1, 0)
    moveTableRow({ from: 1, to: 2, select: false })(
      editor.state,
      editor.view.dispatch
    )
    expect(markdownOf(editor)).toBe(`| a |\n| --- |\n| 2 |\n| 1 |`)
    editor.destroy()
  })

  it(`moves a column right, alignment and header included`, () => {
    const editor = makeEditor(
      `| l | r |\n| :--- | ---: |\n| 1 | 2 |`
    )
    selectCell(editor, 0, 0)
    moveTableColumn({ from: 0, to: 1, select: false })(
      editor.state,
      editor.view.dispatch
    )
    expect(markdownOf(editor)).toBe(`| r | l |\n| ---: | :--- |\n| 2 | 1 |`)
    editor.destroy()
  })
})

// ── menu model ──

describe(`tableMenuModel`, () => {
  it(`protects the header row`, () => {
    const model = tableMenuModel({ row: 0, col: 0, width: 2, height: 3 })
    expect(model.isHeaderRow).toBe(true)
    expect(model.row).toEqual({
      canInsertAbove: false,
      canInsertBelow: true,
      canMoveUp: false,
      canMoveDown: false,
      canDelete: false,
    })
  })

  it(`bounds the moves`, () => {
    const first = tableMenuModel({ row: 1, col: 0, width: 3, height: 3 })
    expect(first.row.canMoveUp).toBe(false)
    expect(first.row.canMoveDown).toBe(true)
    expect(first.column.canMoveLeft).toBe(false)
    expect(first.column.canMoveRight).toBe(true)

    const last = tableMenuModel({ row: 2, col: 2, width: 3, height: 3 })
    expect(last.row.canMoveUp).toBe(true)
    expect(last.row.canMoveDown).toBe(false)
    expect(last.column.canMoveLeft).toBe(true)
    expect(last.column.canMoveRight).toBe(false)
  })

  it(`refuses to delete the last column, allows any body row`, () => {
    expect(
      tableMenuModel({ row: 1, col: 0, width: 1, height: 2 }).column.canDelete
    ).toBe(false)
    expect(
      tableMenuModel({ row: 1, col: 0, width: 2, height: 2 }).column.canDelete
    ).toBe(true)
    expect(
      tableMenuModel({ row: 1, col: 0, width: 2, height: 2 }).row.canDelete
    ).toBe(true)
  })
})
