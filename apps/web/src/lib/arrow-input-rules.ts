import { Extension, textInputRule } from "@tiptap/core"

// Typed-arrow replacement (EXP-307): the desktop IDE's editor font ligates
// `=>` / `->` into single arrow glyphs; the web editor gets the same
// affordance as real text — typing the two-character sequence replaces it
// with the arrow character. Plain unicode text, so the GFM interchange
// contract is untouched, and TipTap input rules already skip code blocks and
// inline-code marks (arrows in code stay literal).
export const ArrowInputRules = Extension.create({
  name: `arrowInputRules`,

  addInputRules() {
    return [
      textInputRule({ find: /=>$/, replace: `⇒` }),
      textInputRule({ find: /->$/, replace: `→` }),
    ]
  },
})
