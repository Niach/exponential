import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Markdown } from "tiptap-markdown"
import { toIssueDescription } from "@exp/db-schema/domain"
import { MarkdownParagraph } from "@/components/issue-editor/markdown-paragraph"

// Drives the real TipTap ↔ markdown pipeline the create/edit paths use
// (markdown-editor.tsx `getMarkdown()` → `toIssueDescription`). The extension
// stack mirrors markdown-editor.tsx: StarterKit with its stock paragraph
// swapped for MarkdownParagraph, which is what makes intentional blank lines
// survive GFM (EXP-7).
//
// The GFM contract (see markdown-paragraph.ts for the full story): raw blank-
// line runs cannot survive a GFM reparse — `A\n\n\n\nB` is byte-for-byte
// indistinguishable from `A\n\nB` to every conforming parser — so an
// intentional empty paragraph is stored as an `&nbsp;` line (a core-CommonMark
// entity, not raw HTML). Every client (markdown-it, cmark-gfm, commonmark-java,
// pulldown-cmark) parses that as a paragraph containing U+00A0, i.e. a
// visually blank line.
function makeEditor(content: unknown) {
  return new Editor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
        paragraph: false,
      }),
      MarkdownParagraph,
      Markdown.configure({
        html: false,
        transformPastedText: true,
        transformCopiedText: true,
      }),
    ],
    content: content as string,
  })
}

function getMarkdown(editor: Editor): string {
  return (
    editor.storage as unknown as { markdown: { getMarkdown: () => string } }
  ).markdown.getMarkdown()
}

const doc = (...content: unknown[]) => ({ type: `doc`, content })
const para = (text?: string) =>
  text === undefined
    ? { type: `paragraph` }
    : { type: `paragraph`, content: [{ type: `text`, text }] }

/** Serialize a doc, push it through the server normalizer, reparse, reserialize. */
function throughCreate(content: unknown) {
  const editor = makeEditor(content)
  const serialized = getMarkdown(editor)
  const stored = toIssueDescription(serialized) ?? ``
  const reparsed = makeEditor(stored)
  const reparsedDoc = reparsed.getJSON()
  const reserialized = getMarkdown(reparsed)
  editor.destroy()
  reparsed.destroy()
  return { serialized, stored, reparsedDoc, reserialized }
}

describe(`markdown newline round-trip`, () => {
  // The base contract: a paragraph break (single Enter) survives the create
  // path and stays a distinct paragraph on reparse, canonically separated by
  // a single blank line.
  it(`preserves a paragraph break through create → render`, () => {
    const { serialized, stored, reserialized } = throughCreate(
      doc(para(`First`), para(`Second`))
    )
    expect(serialized).toBe(`First\n\nSecond`)
    expect(stored).toBe(`First\n\nSecond`)
    expect(reserialized).toBe(`First\n\nSecond`)
  })

  // EXP-7: double-Enter inserts an empty paragraph — intentional vertical
  // spacing. It persists as an `&nbsp;` line and reparses back into a truly
  // empty paragraph (no invisible characters in the editor doc).
  it(`persists an intentional blank line through create → save → reload`, () => {
    const { serialized, stored, reparsedDoc, reserialized } = throughCreate(
      doc(para(`First`), para(), para(`Second`))
    )
    expect(serialized).toBe(`First\n\n&nbsp;\n\nSecond`)
    expect(stored).toBe(`First\n\n&nbsp;\n\nSecond`)
    expect(reserialized).toBe(`First\n\n&nbsp;\n\nSecond`)
    expect(reparsedDoc).toEqual(
      doc(para(`First`), para(), para(`Second`))
    )
  })

  it(`persists multiple consecutive blank lines`, () => {
    const { stored, reparsedDoc, reserialized } = throughCreate(
      doc(para(`First`), para(), para(), para(`Second`))
    )
    expect(stored).toBe(`First\n\n&nbsp;\n\n&nbsp;\n\nSecond`)
    expect(reserialized).toBe(stored)
    expect(reparsedDoc).toEqual(
      doc(para(`First`), para(), para(), para(`Second`))
    )
  })

  // Leading/trailing empty paragraphs are not meaningful spacing — they are
  // dropped, matching the server-side trim of the stored description.
  it(`drops leading and trailing empty paragraphs`, () => {
    const { stored, reserialized } = throughCreate(
      doc(para(), para(`Only line`), para())
    )
    expect(stored).toBe(`Only line`)
    expect(reserialized).toBe(`Only line`)
  })

  // The hard GFM limit this design works around: RAW blank-line runs in
  // markdown source always collapse to a single block boundary on any
  // conforming parser — only `&nbsp;` paragraphs carry extra spacing.
  it(`collapses raw blank-line runs in pasted/stored markdown (GFM limit)`, () => {
    const editor = makeEditor(`First\n\n\n\nSecond`)
    expect(getMarkdown(editor)).toBe(`First\n\nSecond`)
    editor.destroy()
  })

  // Interop: other clients that reserialize our `&nbsp;` paragraph emit the
  // decoded literal U+00A0 instead of the entity. Both forms parse to the
  // same empty paragraph here and converge back to `&nbsp;` on the next save.
  it(`converges a literal U+00A0 blank paragraph to the &nbsp; form`, () => {
    const nbsp = String.fromCharCode(160)
    const editor = makeEditor(`First\n\n${nbsp}\n\nSecond`)
    expect(editor.getJSON()).toEqual(
      doc(para(`First`), para(), para(`Second`))
    )
    expect(getMarkdown(editor)).toBe(`First\n\n&nbsp;\n\nSecond`)
    editor.destroy()
  })

  // Idempotency guard — a second pass must equal the first (no drift toward
  // collapse or growth).
  it(`is idempotent across a second create pass`, () => {
    const first = throughCreate(doc(para(`First`), para(), para(`Second`)))
    const second = throughCreate(first.reparsedDoc)
    expect(second.reserialized).toBe(first.reserialized)
  })

  // Blank lines survive inside container blocks too.
  it(`persists a blank line inside a blockquote`, () => {
    const { stored, reserialized } = throughCreate(
      doc({ type: `blockquote`, content: [para(`a`), para(), para(`b`)] })
    )
    expect(stored).toBe(`> a\n>\n> &nbsp;\n>\n> b`)
    expect(reserialized).toBe(stored)
  })

  // Shift+Enter (hardBreak) is the in-paragraph line break; it uses a distinct
  // GFM representation (`\`-newline) and must survive unchanged.
  it(`preserves a hard break inside a paragraph`, () => {
    const editor = makeEditor(
      doc({
        type: `paragraph`,
        content: [
          { type: `text`, text: `line one` },
          { type: `hardBreak` },
          { type: `text`, text: `line two` },
        ],
      })
    )
    const serialized = getMarkdown(editor)
    editor.destroy()
    expect(serialized).toBe(`line one\\\nline two`)
    const reparsed = makeEditor(serialized)
    expect(getMarkdown(reparsed)).toBe(`line one\\\nline two`)
    reparsed.destroy()
  })
})
