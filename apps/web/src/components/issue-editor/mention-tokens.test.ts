import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Markdown } from "tiptap-markdown"
import { IssueRefExtension } from "@/lib/issue-ref-extension"
import { MentionPillExtension } from "@/lib/mention-pill-extension"
import {
  EditorAutocompleteExtension,
  type EditorAutocompleteActive,
} from "@/lib/editor-autocomplete"

// Drives the real TipTap ↔ markdown pipeline of markdown-editor.tsx with the
// full token-extension stack (issue-ref pills, mention pills, the caret
// autocomplete) to lock the interchange contract: `@<email>` mentions and
// `#<IDENTIFIER>` issue references are PLAIN GFM TEXT — the pills are pure
// decorations and the autocomplete inserts plain text, so both tokens must
// round-trip byte-identically through serialization.

function makeEditor(content: string) {
  const states: (EditorAutocompleteActive | null)[] = []
  const editor = new Editor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
      IssueRefExtension.configure({
        // Resolve everything so the pill decorations are actually active.
        getResolved: () => ({ title: `Some issue` }),
        onOpen: () => {},
      }),
      MentionPillExtension.configure({
        getResolved: () => ({ name: `Ada Lovelace` }),
      }),
      EditorAutocompleteExtension.configure({
        onStateChange: (active) => states.push(active),
      }),
      Markdown.configure({
        html: false,
        transformPastedText: true,
        transformCopiedText: true,
      }),
    ],
    content,
  })
  return { editor, states }
}

function getMarkdown(editor: Editor): string {
  return (
    editor.storage as unknown as { markdown: { getMarkdown: () => string } }
  ).markdown.getMarkdown()
}

function roundTrip(markdown: string): string {
  const { editor } = makeEditor(markdown)
  const serialized = getMarkdown(editor)
  editor.destroy()
  return serialized
}

/** Caret position at the very end of the (single-textblock) document. */
function endOfDoc(editor: Editor): number {
  return editor.state.doc.content.size - 1
}

describe(`mention/issue-ref markdown round-trip`, () => {
  it(`keeps @email mentions as plain text`, () => {
    const src = `Ping @ada@example.com about the rollout`
    expect(roundTrip(src)).toBe(src)
  })

  it(`keeps #IDENTIFIER issue refs as plain text`, () => {
    const src = `Duplicate of #EXP-42, see also #EXP-7`
    expect(roundTrip(src)).toBe(src)
  })

  it(`round-trips tokens at line start (no heading/escape drift)`, () => {
    expect(roundTrip(`#EXP-42 needs a second look`)).toBe(
      `#EXP-42 needs a second look`
    )
    expect(roundTrip(`@ada@example.com owns this`)).toBe(
      `@ada@example.com owns this`
    )
  })

  it(`round-trips both tokens across paragraphs`, () => {
    const src = `Intro for @ada@example.com\n\nRelates to #EXP-42`
    expect(roundTrip(src)).toBe(src)
  })

  it(`is idempotent across a second pass`, () => {
    const once = roundTrip(`Ping @ada@example.com about #EXP-42`)
    expect(roundTrip(once)).toBe(once)
  })
})

describe(`editor autocomplete`, () => {
  it(`reports an in-progress @mention at the caret`, () => {
    const { editor, states } = makeEditor(`Hello @ad`)
    editor.commands.setTextSelection(endOfDoc(editor))
    expect(states.at(-1)).toMatchObject({ kind: `mention`, query: `ad` })
    editor.destroy()
  })

  it(`reports an in-progress #issue ref at the caret`, () => {
    const { editor, states } = makeEditor(`Fixes #EX`)
    editor.commands.setTextSelection(endOfDoc(editor))
    expect(states.at(-1)).toMatchObject({ kind: `issueRef`, query: `EX` })
    editor.destroy()
  })

  it(`inserts the plain @<email> interchange text on selection`, () => {
    const { editor, states } = makeEditor(`Hello @ad`)
    editor.commands.setTextSelection(endOfDoc(editor))
    const active = states.at(-1)
    expect(active).not.toBeNull()
    // Mirrors markdown-editor.tsx insertToken: plain insertText, never a node.
    editor
      .chain()
      .command(({ tr }) => {
        tr.insertText(`@ada@example.com `, active!.from, active!.to)
        return true
      })
      .run()
    expect(getMarkdown(editor).trimEnd()).toBe(`Hello @ada@example.com`)
    // The completed token no longer matches an in-progress trigger.
    expect(states.at(-1)).toBeNull()
    editor.destroy()
  })

  it(`inserts the plain #<IDENTIFIER> interchange text on selection`, () => {
    const { editor, states } = makeEditor(`Fixes #EX`)
    editor.commands.setTextSelection(endOfDoc(editor))
    const active = states.at(-1)
    expect(active).not.toBeNull()
    editor
      .chain()
      .command(({ tr }) => {
        tr.insertText(`#EXP-42 `, active!.from, active!.to)
        return true
      })
      .run()
    expect(getMarkdown(editor).trimEnd()).toBe(`Fixes #EXP-42`)
    expect(states.at(-1)).toBeNull()
    editor.destroy()
  })

  it(`does not trigger mid-word or inside code blocks`, () => {
    const glued = makeEditor(`hello@ad`)
    glued.editor.commands.setTextSelection(endOfDoc(glued.editor))
    expect(glued.states.at(-1) ?? null).toBeNull()
    glued.editor.destroy()

    const code = makeEditor(`\`\`\`\nhello @ad\n\`\`\``)
    code.editor.commands.setTextSelection(endOfDoc(code.editor))
    expect(code.states.at(-1) ?? null).toBeNull()
    code.editor.destroy()
  })
})
