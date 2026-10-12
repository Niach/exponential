import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Markdown } from "tiptap-markdown"
import { IssueRefExtension } from "@/lib/issue-ref-extension"
import { MentionPillExtension } from "@/lib/mention-pill-extension"
import {
  EditorAutocompleteExtension,
  FORCE_ISSUE_REF_META,
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
        getResolved: () => ({
          title: `Some issue`,
          statusIcon: `circle`,
          statusColor: `var(--foreground)`,
        }),
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

/** Simulate typing `text` at the end of the document — the autocomplete only
 *  OPENS on doc changes (EXP-307), so tests must type like a user instead of
 *  merely parking the caret after pre-loaded content. */
function typeAtEnd(editor: Editor, text: string) {
  editor.commands.setTextSelection(endOfDoc(editor))
  editor.commands.insertContent(text)
}

describe(`editor autocomplete`, () => {
  it(`reports an in-progress @mention at the caret`, () => {
    const { editor, states } = makeEditor(`Hello`)
    typeAtEnd(editor, ` @ad`)
    expect(states.at(-1)).toMatchObject({ kind: `mention`, query: `ad` })
    editor.destroy()
  })

  it(`reports an in-progress #issue ref at the caret`, () => {
    const { editor, states } = makeEditor(`Fixes`)
    typeAtEnd(editor, ` #EX`)
    expect(states.at(-1)).toMatchObject({ kind: `issueRef`, query: `EX` })
    editor.destroy()
  })

  it(`does not open on a pure caret move onto an existing token`, () => {
    // Clicking right after a complete `#EXP-42` used to pop the picker
    // instead of leaving the chip clickable (EXP-307): the caret lands where
    // the at-caret regex matches, but nothing was typed.
    const { editor, states } = makeEditor(`Fixes #EXP-42`)
    editor.commands.setTextSelection(endOfDoc(editor))
    expect(states.at(-1) ?? null).toBeNull()

    // Typing right there still opens it.
    editor.commands.insertContent(`3`)
    expect(states.at(-1)).toMatchObject({ kind: `issueRef`, query: `EXP-423` })
    editor.destroy()
  })

  it(`inserts the plain @<email> interchange text on selection`, () => {
    const { editor, states } = makeEditor(`Hello`)
    typeAtEnd(editor, ` @ad`)
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
    const { editor, states } = makeEditor(`Fixes`)
    typeAtEnd(editor, ` #EX`)
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
    const glued = makeEditor(`hello`)
    typeAtEnd(glued.editor, `@ad`)
    expect(glued.states.at(-1) ?? null).toBeNull()
    glued.editor.destroy()

    const code = makeEditor(`\`\`\`\nhello\n\`\`\``)
    typeAtEnd(code.editor, ` @ad`)
    expect(code.states.at(-1) ?? null).toBeNull()
    code.editor.destroy()
  })
})

// Review #5 (search-list.json): typed key by key through the editor's text
// input handlers (where the heading input rule lives), a line-start `#`
// waits for a token character, so `# Title` becomes a heading.
function typeKeys(editor: Editor, text: string) {
  for (const ch of text) {
    const { from, to } = editor.state.selection
    const view = editor.view
    const handled = view.someProp(`handleTextInput`, (handler) =>
      handler(view, from, to, ch, () => view.state.tr.insertText(ch, from, to))
    )
    if (!handled) view.dispatch(view.state.tr.insertText(ch, from, to))
  }
}

describe(`the line-start # (review #5)`, () => {
  it(`a typed "# Title" becomes a heading and never opens the search`, () => {
    const { editor, states } = makeEditor(``)
    editor.commands.setTextSelection(1)
    typeKeys(editor, `# Title`)
    expect(states.some((state) => state?.kind === `issueRef`)).toBe(false)
    expect(editor.state.doc.firstChild?.type.name).toBe(`heading`)
    expect(getMarkdown(editor)).toBe(`# Title`)
    editor.destroy()
  })

  it(`"## H" stays a heading too`, () => {
    const { editor, states } = makeEditor(``)
    editor.commands.setTextSelection(1)
    typeKeys(editor, `## H`)
    expect(states.some((state) => state?.kind === `issueRef`)).toBe(false)
    expect(editor.state.doc.firstChild?.attrs.level).toBe(2)
    editor.destroy()
  })

  it(`a line-start "#E" opens with its token`, () => {
    const { editor, states } = makeEditor(``)
    editor.commands.setTextSelection(1)
    typeKeys(editor, `#`)
    expect(states.at(-1) ?? null).toBeNull()
    typeKeys(editor, `E`)
    expect(states.at(-1)).toMatchObject({ kind: `issueRef`, query: `E`, from: 1, to: 3 })
    editor.destroy()
  })

  it(`a # after a space opens at once`, () => {
    const { editor, states } = makeEditor(`Fixes`)
    editor.commands.setTextSelection(endOfDoc(editor))
    typeKeys(editor, ` #`)
    expect(states.at(-1)).toMatchObject({ kind: `issueRef`, query: `` })
    editor.destroy()
  })

  it(`the rail's # button opens it on a bare line-start #`, () => {
    const { editor, states } = makeEditor(``)
    editor
      .chain()
      .setTextSelection(1)
      .command(({ tr }) => {
        tr.setMeta(FORCE_ISSUE_REF_META, true)
        tr.insertText(`#`)
        return true
      })
      .run()
    expect(states.at(-1)).toMatchObject({ kind: `issueRef`, query: ``, from: 1, to: 2 })
    editor.destroy()
  })
})
