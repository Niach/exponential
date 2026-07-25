import { describe, expect, it } from "vitest"
import { Editor } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Markdown } from "tiptap-markdown"
import { MarkdownImage } from "@/lib/markdown-image"
import { MarkdownParagraph } from "@/components/issue-editor/markdown-paragraph"

// EXP-271: `@tiptap/extension-image` is a BLOCK node, but tiptap-markdown
// serializes it with prosemirror-markdown's inline serializer — without the
// `closeBlock`, every save welded the image onto the next paragraph
// (`![](/api/attachments/x)after`). That malformed line is still a legal
// paragraph containing an inline image, so web/iOS/Android happened to lift
// it back out — but the desktop WYSIWYG editor renders it as literal text.
// These fixtures lock the block form on the way out.
function makeEditor(markdown: string) {
  return new Editor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] }, paragraph: false }),
      MarkdownParagraph,
      MarkdownImage,
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

describe(`block image markdown round-trip`, () => {
  it.each([
    [`lone image`, `![diagram](/api/attachments/abc123)`],
    [`image then text`, `![alt](/api/attachments/abc)\n\nafter`],
    [`text then image`, `before\n\n![alt](/api/attachments/abc)`],
    [
      `text image text (contract fixture)`,
      `before\n\n![alt](/api/attachments/abc)\n\nafter`,
    ],
    [
      `two images in a row`,
      `![a](/api/attachments/a)\n\n![b](/api/attachments/b)`,
    ],
    [`image with width param`, `![alt](/api/attachments/abc?w=480)\n\nafter`],
  ])(`%s survives byte-for-byte`, (_name, markdown) => {
    expect(roundTrip(markdown)).toBe(markdown)
  })

  it(`splits an image glued into a paragraph back into its own block`, () => {
    // The shape already stored by pre-fix saves: reopening and saving heals it.
    expect(roundTrip(`![](/api/attachments/abc)on mobile steering.`)).toBe(
      `![](/api/attachments/abc)\n\non mobile steering.`,
    )
    expect(roundTrip(`text ![alt](/api/attachments/abc) more`)).toBe(
      `text\n\n![alt](/api/attachments/abc)\n\nmore`,
    )
  })
})
