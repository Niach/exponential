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
import {
  MarkdownTableExtensions,
  moveSelectionAfterTable,
  restoreTableCellImages,
  tableTrailingNodeOptions,
} from "@/lib/markdown-table"
import { extractMarkdownImageOccurrences } from "@/lib/storage/issue-attachments"
import { tableMenuModel } from "@/components/issue-editor/table-controls"

// EXP-726 — GFM tables travel through `issues.description` / `comments.body`
// and must come back BYTE-identical on every client. These fixtures are the
// shared cross-client corpus, mirrored in the desktop CONTRACT_FIXTURES, the
// Android MarkdownRoundTripTest and the iOS table suite.
function makeEditor(markdown: string, editable = true) {
  return new Editor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
        paragraph: false,
        trailingNode: tableTrailingNodeOptions,
      }),
      MarkdownParagraph,
      MarkdownImage,
      ...MarkdownTableExtensions,
      Markdown.configure({ html: false }),
    ],
    content: markdown,
    editable,
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
  // keeps its literal markdown text (the cross-client contract) — UNescaped
  // on the way back out, byte-identical to what the other clients emit and
  // still a match for the attachment scanner.
  it(`keeps an image inside a cell as literal text`, () => {
    const markdown = `| ![chart](/api/attachments/abc) | notes |\n| --- | --- |\n| 1 | 2 |`
    expect(roundTrip(markdown)).toBe(markdown)
    expect(roundTrip(roundTrip(markdown))).toBe(markdown)
  })

  // The escaped form `!\[alt\](src)` is invisible to the attachment scanner,
  // so the image would count as removed and its attachment be reclaimed.
  it(`leaves an in-cell image visible to the attachment scanner`, () => {
    const id = `3f1b9c2e-4a5d-4e6f-8a9b-0c1d2e3f4a5b`
    const stored = roundTrip(
      `| ![chart](/api/attachments/${id}) | notes |\n| --- | --- |\n| 1 | 2 |`
    )
    expect(
      extractMarkdownImageOccurrences(stored).map((occurrence) => [
        occurrence.alt,
        occurrence.url,
      ])
    ).toEqual([[`chart`, `/api/attachments/${id}`]])
  })

  // Kept OUT of the shared corpus (only web's serializer is verified here):
  // a literal backslash in front of a pipe. The backslash run doubles, then
  // the pipe takes its own escape.
  it(`escapes a literal backslash before a pipe`, () => {
    const markdown = `| a\\\\\\|b | c |\n| --- | --- |\n| 1 | 2 |`
    expect(roundTrip(markdown)).toBe(markdown)
    expect(roundTrip(roundTrip(markdown))).toBe(markdown)
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

describe(`restoreTableCellImages`, () => {
  it(`unescapes only the image's own brackets`, () => {
    expect(
      restoreTableCellImages(`a \\*b\\* !\\[chart\\](/api/attachments/x) c`)
    ).toBe(`a \\*b\\* ![chart](/api/attachments/x) c`)
  })

  // Conservative on purpose: an alt carrying its own bracket is ambiguous
  // against ordinary escaped text, so it keeps the escape rather than risk
  // making a link out of it.
  it(`leaves a bracketed alt escaped`, () => {
    const text = `!\\[shot \\[1\\].png\\](/api/attachments/x)`
    expect(restoreTableCellImages(text)).toBe(text)
  })

  it(`leaves escaped brackets that are not an image alone`, () => {
    const text = `!\\[not an image\\] and \\[a\\](/b)`
    expect(restoreTableCellImages(text)).toBe(text)
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

  // A read-only body has no caret to rescue, and the host dispatches an empty
  // transaction on mount — an ungated plugin would show a blank row under it.
  it(`adds no trailing paragraph to a read-only table`, () => {
    const editor = makeEditor(`| a |\n| --- |\n| 1 |`, false)
    editor.view.dispatch(editor.state.tr)
    expect(editor.state.doc.childCount).toBe(1)
    expect(editor.state.doc.child(0).type.name).toBe(`table`)
    // ...and it comes back the moment the editor turns editable.
    editor.setEditable(true)
    editor.view.dispatch(editor.state.tr)
    expect(editor.state.doc.childCount).toBe(2)
    editor.destroy()
  })
})

// ── image insertion ──

describe(`moveSelectionAfterTable`, () => {
  it(`drops an image below the table instead of splitting it`, () => {
    const editor = makeEditor(`| a | b |\n| --- | --- |\n| 1 | 2 |`)
    selectCell(editor, 1, 0)
    expect(moveSelectionAfterTable(editor)).toBe(true)
    editor.commands.setImage({ alt: `alt`, src: `/api/attachments/x` })
    expect(markdownOf(editor)).toBe(
      `| a | b |\n| --- | --- |\n| 1 | 2 |\n\n![alt](/api/attachments/x)`
    )
    editor.destroy()
  })

  it(`leaves a selection outside a table alone`, () => {
    const editor = makeEditor(`before\n\n| a |\n| --- |\n| 1 |`)
    editor.commands.focus(`start`)
    expect(moveSelectionAfterTable(editor)).toBe(false)
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

// EXP-728 — nested tables are NOT supported. A table is a top-level block:
// the three native flat models (desktop comrak, iOS, Android) HOIST one found
// inside a list item or blockquote out to the document level on parse, and
// the hoisted form is the canonical one. These strings are byte-mirrored in
// the desktop TABLE_HOIST_FIXTURES, the iOS table suite and the Android
// MarkdownRoundTripTest — add a fixture in all four or in none.
//
// Web is deliberately NOT a normalizer: it keeps the nesting it is handed.
// Forbidding `table` inside listItem/blockquote in the schema would make the
// ProseMirror DOMParser hoist too, but its rewrap of the split list's tail is
// lossy (list type and nesting depth) — real data mangling for a construct
// nobody wants. Ordered lists stay out of the
// corpus: how the tail item is renumbered after a hoist is engine-specific.
describe(`nested tables (EXP-728)`, () => {
  const hoists: Array<[string, string, string]> = [
    [
      `table_hoisted_from_list`,
      `- step one\n\n  | a | b |\n  | --- | --- |\n  | 1 | 2 |\n\n- step two`,
      `- step one\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n- step two`,
    ],
    [
      `table_hoisted_from_quote`,
      `> intro\n>\n> | a | b |\n> | --- | --- |\n> | 1 | 2 |`,
      `> intro\n\n| a | b |\n| --- | --- |\n| 1 | 2 |`,
    ],
  ]

  it.each(hoists)(
    `%s: the hoisted form the natives converge on is a fixpoint here`,
    (_name, _nested, hoisted) => {
      expect(roundTrip(hoisted)).toBe(hoisted)
    }
  )

  // Structure, not bytes: web's own serializer owns the spacer lines, so
  // only the nesting itself is pinned here.
  it.each(hoists)(
    `%s: web keeps the table inside its list item / blockquote`,
    (_name, nested) => {
      const editor = makeEditor(nested)
      const parents: string[] = []
      editor.state.doc.descendants((node, pos) => {
        if (node.type.name === `table`) {
          parents.push(editor.state.doc.resolve(pos).parent.type.name)
        }
      })
      editor.destroy()
      expect(parents).toHaveLength(1)
      expect([`listItem`, `blockquote`]).toContain(parents[0])
    }
  )
})
