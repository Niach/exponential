import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Markdown } from "tiptap-markdown"
import {
  EditorAutocompleteExtension,
  type EditorAutocompleteActive,
} from "@/lib/editor-autocomplete"

// EXP-551 — the `:shortcode` typeahead in the TipTap editor and the emoji
// markdown contract: pickers insert UNICODE, which round-trips through
// tiptap-markdown byte-identically; a literal `:shortcode:` in stored text is
// plain text and stays exactly that (no client expands shortcodes).
// Sibling of mention-tokens.test.ts.

function makeEditor(content: string) {
  const states: (EditorAutocompleteActive | null)[] = []
  const editor = new Editor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
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

function endOfDoc(editor: Editor): number {
  return editor.state.doc.content.size - 1
}

function typeAtEnd(editor: Editor, text: string) {
  editor.commands.setTextSelection(endOfDoc(editor))
  editor.commands.insertContent(text)
}

describe(`emoji markdown round-trip`, () => {
  it(`keeps unicode emoji byte-identical`, () => {
    for (const src of [
      `Ship it 🎉`,
      `👍🏽 looks good`,
      `family 👨‍👩‍👧‍👦 and flag 🇦🇹`,
      `- done ✅\n- todo 🚧`,
      `**bold 😄** and \`code 🐛\``,
    ]) {
      expect(roundTrip(src)).toBe(src)
    }
  })

  it(`leaves a literal :shortcode: alone`, () => {
    expect(roundTrip(`Ship it :tada:`)).toBe(`Ship it :tada:`)
  })
})

describe(`:emoji autocomplete`, () => {
  it(`reports an in-progress :shortcode after two chars`, () => {
    const { editor, states } = makeEditor(`Ship it`)
    typeAtEnd(editor, ` :t`)
    expect(states.at(-1) ?? null).toBeNull()
    editor.commands.insertContent(`a`)
    const active = states.at(-1)
    expect(active).toMatchObject({ kind: `emoji`, query: `ta`, closed: false })
    // from = the colon, to = the caret
    expect(active!.to - active!.from).toBe(3)
    editor.destroy()
  })

  it(`keeps reporting through the closing colon (closed) with the full span`, () => {
    const { editor, states } = makeEditor(`Ship it`)
    typeAtEnd(editor, ` :tada:`)
    const active = states.at(-1)
    expect(active).toMatchObject({ kind: `emoji`, query: `tada`, closed: true })
    expect(active!.to - active!.from).toBe(6)
    // Replacing the whole token with the unicode (what the auto-commit does)
    // yields the emoji and closes the menu.
    editor
      .chain()
      .command(({ tr }) => {
        tr.insertText(`🎉`, active!.from, active!.to)
        return true
      })
      .run()
    expect(getMarkdown(editor).trimEnd()).toBe(`Ship it 🎉`)
    expect(states.at(-1)).toBeNull()
    editor.destroy()
  })

  it(`inserts unicode + space on a menu pick`, () => {
    const { editor, states } = makeEditor(`Ship it`)
    typeAtEnd(editor, ` :tad`)
    const active = states.at(-1)!
    editor
      .chain()
      .command(({ tr }) => {
        tr.insertText(`🎉 `, active.from, active.to)
        return true
      })
      .run()
    expect(getMarkdown(editor)).toBe(`Ship it 🎉 `)
    editor.destroy()
  })

  it(`does not trigger on times, one char, mid-word, or inside code`, () => {
    const time = makeEditor(`meet at`)
    typeAtEnd(time.editor, ` 12:30`)
    expect(time.states.at(-1) ?? null).toBeNull()
    time.editor.destroy()

    const glued = makeEditor(`note`)
    typeAtEnd(glued.editor, `:fix`)
    expect(glued.states.at(-1) ?? null).toBeNull()
    glued.editor.destroy()

    const code = makeEditor(`\`\`\`\nhello\n\`\`\``)
    typeAtEnd(code.editor, ` :ta`)
    expect(code.states.at(-1) ?? null).toBeNull()
    code.editor.destroy()
  })

  it(`still prefers @ and # when both could match`, () => {
    const { editor, states } = makeEditor(`Ping`)
    typeAtEnd(editor, ` @ad`)
    expect(states.at(-1)).toMatchObject({ kind: `mention` })
    editor.destroy()
  })
})
