import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Markdown } from "tiptap-markdown"
import { toIssueDescription } from "@exp/db-schema/domain"

// Drives the real TipTap ↔ markdown pipeline the create/edit paths use
// (markdown-editor.tsx `getMarkdown()` → `toIssueDescription`). The Markdown
// config here mirrors markdown-editor.tsx:411-415.
function makeEditor(content: unknown) {
  return new Editor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
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
  const reserialized = getMarkdown(reparsed)
  editor.destroy()
  reparsed.destroy()
  return { serialized, stored, reserialized }
}

describe(`markdown newline round-trip`, () => {
  // The core contract: a paragraph break (single Enter) MUST survive the
  // create path and stay a distinct paragraph on reparse — this is the
  // "visible paragraph gap that round-trips" (EXP-5 #6). Canonical form is a
  // single blank line, byte-identical to the Android/iOS lock
  // (MarkdownRoundTripTest.multipleParagraphs).
  it(`preserves a paragraph break through create → render`, () => {
    const { serialized, stored, reserialized } = throughCreate(
      doc(para(`First`), para(`Second`))
    )
    expect(serialized).toBe(`First\n\nSecond`)
    expect(stored).toBe(`First\n\nSecond`)
    expect(reserialized).toBe(`First\n\nSecond`)
  })

  // Double-Enter inserts an empty paragraph. Per the GFM spec consecutive
  // blank lines collapse, and the cross-client canonical form (Android byte-
  // parity lock) is a SINGLE blank line between blocks — so the empty
  // paragraph canonically collapses to one paragraph break. This is the
  // contract-correct outcome; it must NOT be "fixed" by emitting extra blank
  // lines or a synthetic blank paragraph (that would break byte-parity). The
  // surviving break still round-trips and renders as a real gap.
  it(`collapses an empty paragraph to the canonical single break`, () => {
    const { serialized, stored, reserialized } = throughCreate(
      doc(para(`First`), para(), para(`Second`))
    )
    expect(serialized).toBe(`First\n\nSecond`)
    expect(stored).toBe(`First\n\nSecond`)
    expect(reserialized).toBe(`First\n\nSecond`)
  })

  it(`collapses multiple empty paragraphs the same way`, () => {
    const { reserialized } = throughCreate(
      doc(para(`First`), para(), para(), para(`Second`))
    )
    expect(reserialized).toBe(`First\n\nSecond`)
  })

  // Idempotency guard mirroring MarkdownRoundTripTest.idempotentMixed — a
  // second pass must equal the first (no drift toward collapse or growth).
  it(`is idempotent across a second create pass`, () => {
    const first = throughCreate(doc(para(`First`), para(), para(`Second`)))
    const second = throughCreate(makeEditor(first.stored).getJSON())
    expect(second.reserialized).toBe(first.reserialized)
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
