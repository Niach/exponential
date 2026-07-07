import { Paragraph } from "@tiptap/extension-paragraph"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"
import type { MarkdownNodeSpec } from "tiptap-markdown"

// EXP-7 — intentional blank lines must survive save + reload.
//
// GFM/CommonMark cannot represent an empty paragraph with raw newlines alone:
// any run of blank lines between two blocks parses as a SINGLE block boundary
// (CommonMark "blank lines"), so `A\n\n\n\nB` reparses identically to
// `A\n\nB` on every conforming parser — markdown-it here, cmark-gfm on iOS,
// commonmark-java on Android, pulldown-cmark on desktop. That is the hard GFM
// limit: extra blank lines written as plain newlines are always lossy.
//
// The GFM-sanctioned escape hatch (the same one used on GitHub itself) is a
// paragraph whose only content is a no-break space, written as the `&nbsp;`
// character entity. Entity references are core CommonMark text — NOT raw
// HTML — so they survive our `html: false` config and parse on all four
// clients as a paragraph containing U+00A0, i.e. a visually blank line.
//
// This extension therefore:
// - serializes every INTERIOR empty paragraph as an `&nbsp;` line
//   (`First\n\n&nbsp;\n\nSecond`), preserving as many intentional blank
//   lines as the user created — one `&nbsp;` paragraph per blank line;
// - drops leading/trailing empty paragraphs (they are not meaningful
//   spacing, and the server trims the stored description anyway);
// - on parse, folds whitespace/U+00A0-only `<p>`s back into truly empty
//   paragraphs, so the editor shows a real blank line with no invisible
//   characters and the stored form stays byte-stable across edit cycles
//   (parse(`&nbsp;`) → empty paragraph → serialize(`&nbsp;`)). Literal
//   U+00A0 paragraphs written by the other clients converge to the same
//   `&nbsp;` form on the next web save.
export const nbspEntity = `&nbsp;`

/** Empty, or containing only whitespace text (JS `\s` includes U+00A0). */
function isBlankParagraph(node: ProseMirrorNode): boolean {
  if (node.childCount === 0) {
    return true
  }
  for (let index = 0; index < node.childCount; index++) {
    const child = node.child(index)
    if (!child.isText || !/^\s*$/.test(child.text ?? ``)) {
      return false
    }
  }
  return true
}

export const MarkdownParagraph = Paragraph.extend({
  addStorage() {
    return {
      markdown: {
        serialize(state, node, parent, index) {
          if (isBlankParagraph(node)) {
            const interior = index > 0 && index < parent.childCount - 1
            if (interior) {
              state.write(nbspEntity)
              state.closeBlock(node)
            }
            return
          }
          // Default prosemirror-markdown paragraph serialization.
          state.renderInline(node)
          state.closeBlock(node)
        },
        parse: {
          updateDOM(element) {
            // markdown-it renders an `&nbsp;` blank-line paragraph as
            // `<p>&#160;</p>` (a lone U+00A0 text node) — strip the
            // placeholder so the editor holds a genuinely empty paragraph.
            element.querySelectorAll(`p`).forEach((paragraph) => {
              if (
                paragraph.childElementCount === 0 &&
                /^\s*$/.test(paragraph.textContent ?? ``)
              ) {
                paragraph.textContent = ``
              }
            })
          },
        },
      } satisfies MarkdownNodeSpec,
    }
  },
})
