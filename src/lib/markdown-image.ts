import Image from "@tiptap/extension-image"

export const MarkdownImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      alt: {
        default: null,
      },
      src: {
        default: null,
      },
      title: {
        default: null,
      },
    }
  },
}).configure({
  allowBase64: false,
  HTMLAttributes: {
    class: `editor-image`,
  },
})
