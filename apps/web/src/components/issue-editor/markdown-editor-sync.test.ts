import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Markdown } from "tiptap-markdown"
import { MarkdownParagraph } from "@/components/issue-editor/markdown-paragraph"

// Locks the TipTap contracts the description/comment remote-sync plumbing is
// built on (markdown-editor.tsx + issue-detail-view.tsx). Descriptions can be
// authored by any client (native apps, MCP, the widget's raw textarea), so
// the synced text need not round-trip byte-identically through this editor —
// the sync bookkeeping therefore settles its unsaved-edits baseline from the
// editor's own serialization, and the read-only re-apply path must never
// re-enter onChange with re-serialized text.
function makeEditor(content: string, onUpdate?: (markdown: string) => void) {
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
    content,
    onUpdate: ({ editor }) => {
      onUpdate?.(getMarkdown(editor))
    },
  })
}

function getMarkdown(editor: Editor): string {
  return (
    editor.storage as unknown as { markdown: { getMarkdown: () => string } }
  ).markdown.getMarkdown()
}

// GFM that is valid on every client but not TipTap-canonical: soft-wrapped
// lines join, `*` bullets become `-`, `_em_` becomes `*em*`.
const nonCanonical = `soft\nwrap\n\n* star bullet\n\n_emphasis_`

describe(`markdown editor remote-content application`, () => {
  it(`re-serializes common foreign GFM differently than authored`, () => {
    const editor = makeEditor(nonCanonical)
    const canonical = getMarkdown(editor)
    expect(canonical).not.toBe(nonCanonical)
    editor.destroy()
  })

  it(`setContent emits the re-serialized text through onUpdate by default`, () => {
    const updates: string[] = []
    const editor = makeEditor(``, (markdown) => updates.push(markdown))
    editor.commands.setContent(nonCanonical)
    // This emission overwrites the host's local-text ref — which is why the
    // host must baseline against the editor's serialization, never the raw
    // synced string (the two differ here).
    expect(updates).toEqual([getMarkdown(editor)])
    expect(updates[0]).not.toBe(nonCanonical)
    editor.destroy()
  })

  it(`setContent with emitUpdate: false applies content silently`, () => {
    const updates: string[] = []
    const editor = makeEditor(``, (markdown) => updates.push(markdown))
    editor.commands.setContent(nonCanonical, { emitUpdate: false })
    // The read-only re-apply effect relies on this: content lands without
    // pushing a divergent serialization into onChange.
    expect(updates).toEqual([])
    expect(getMarkdown(editor)).not.toBe(``)
    editor.destroy()
  })

  it(`the editor's own serialization is a re-application fixpoint`, () => {
    const editor = makeEditor(nonCanonical)
    const canonical = getMarkdown(editor)
    editor.commands.setContent(canonical)
    // Settling baselines from getMarkdown() is only sound if re-applying
    // that serialization yields itself.
    expect(getMarkdown(editor)).toBe(canonical)
    editor.destroy()
  })
})
