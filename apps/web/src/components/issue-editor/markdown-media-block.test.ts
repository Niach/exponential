import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Link } from "@tiptap/extension-link"
import { Markdown } from "tiptap-markdown"
import { MarkdownImage } from "@/lib/markdown-image"
import { MarkdownMedia, mediaLinkNodeName } from "@/lib/markdown-media"
import { MarkdownParagraph } from "@/components/issue-editor/markdown-paragraph"

// EXP-824: the inline media contract is a PLAIN LINK on a paragraph of its
// own — `[clip.mp4](/api/attachments/{id})` — never the image form. These
// fixtures are mirrored byte-for-byte on iOS, Android and the desktop.
function makeEditor(markdown: string) {
  return new Editor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] }, paragraph: false }),
      MarkdownParagraph,
      Link.configure({ openOnClick: false }),
      MarkdownImage,
      MarkdownMedia,
      Markdown.configure({ html: false }),
    ],
    content: markdown,
  })
}

function roundTrip(markdown: string): string {
  const editor = makeEditor(markdown)
  const out = (
    editor.storage as unknown as { markdown: { getMarkdown: () => string } }
  ).markdown.getMarkdown()
  editor.destroy()
  return out
}

function blockTypes(markdown: string): string[] {
  const editor = makeEditor(markdown)
  const types: string[] = []
  editor.state.doc.forEach((node) => types.push(node.type.name))
  editor.destroy()
  return types
}

describe(`media block markdown round-trip`, () => {
  it.each([
    [`lone clip (contract fixture)`, `[clip.mp4](/api/attachments/abc123)`],
    [
      `text clip text (contract fixture)`,
      `before\n\n[clip.mp4](/api/attachments/abc)\n\nafter`,
    ],
    [
      `clip with width param (web fixture)`,
      `[clip.mp4](/api/attachments/abc?w=480)\n\nafter`,
    ],
    [`clip then text`, `[clip.mp4](/api/attachments/abc)\n\nafter`],
    [`text then clip`, `before\n\n[clip.mp4](/api/attachments/abc)`],
    [
      `two clips in a row`,
      `[a.mp4](/api/attachments/a)\n\n[b.webm](/api/attachments/b)`,
    ],
    [
      `clip beside an image`,
      `![shot](/api/attachments/img)\n\n[clip.mp4](/api/attachments/vid)`,
    ],
    [`audio file`, `[voice-note.m4a](/api/attachments/aud)`],
  ])(`%s survives byte-for-byte`, (_name, markdown) => {
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it(`parses the standalone attachment link as a media block`, () => {
    expect(blockTypes(`[clip.mp4](/api/attachments/abc)`)).toEqual([
      mediaLinkNodeName,
    ])
    expect(
      blockTypes(`before\n\n[clip.mp4](/api/attachments/abc)\n\nafter`)
    ).toEqual([`paragraph`, mediaLinkNodeName, `paragraph`])
  })

  it(`keeps a link inside running text an ordinary link`, () => {
    const markdown = `see [clip.mp4](/api/attachments/abc) for the repro`
    expect(blockTypes(markdown)).toEqual([`paragraph`])
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it(`keeps a standalone link to a non-attachment URL a link`, () => {
    const markdown = `[docs](https://example.com/guide)`
    expect(blockTypes(markdown)).toEqual([`paragraph`])
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it(`keeps a standalone attachment link on a FOREIGN host a link`, () => {
    // The path alone is not ours: only relative or same-origin URLs lift.
    const markdown = `[clip.mp4](https://other-host.example/api/attachments/abc)`
    expect(blockTypes(markdown)).toEqual([`paragraph`])
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it(`lifts an absolute same-origin attachment link`, () => {
    const markdown = `[clip.mp4](${window.location.origin}/api/attachments/abc)`
    expect(blockTypes(markdown)).toEqual([mediaLinkNodeName])
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it(`keeps two links on one paragraph as links`, () => {
    const markdown = `[a.mp4](/api/attachments/a) [b.mp4](/api/attachments/b)`
    expect(blockTypes(markdown)).toEqual([`paragraph`])
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it(`leaves attachment links inside list items alone`, () => {
    const markdown = `- [clip.mp4](/api/attachments/abc)`
    expect(blockTypes(markdown)).toEqual([`bulletList`])
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it(`serializes an inserted block with escaped parentheses in the src`, () => {
    const editor = makeEditor(``)
    editor.commands.setMediaLink({
      label: `clip.mp4`,
      src: `/api/attachments/abc`,
    })
    const out = (
      editor.storage as unknown as { markdown: { getMarkdown: () => string } }
    ).markdown.getMarkdown()
    editor.destroy()
    expect(out).toBe(`[clip.mp4](/api/attachments/abc)`)
  })
})
