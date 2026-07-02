import { Extension } from "@tiptap/core"
import { Plugin, PluginKey } from "@tiptap/pm/state"
import { Decoration, DecorationSet } from "@tiptap/pm/view"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"
import { createIssueRefRegExp } from "@/lib/issue-refs"

// Renders `#IDENTIFIER` issue references as clickable pills via inline
// decorations — the document text stays the plain token, so the GFM markdown
// round-trip is untouched (mirrors how `@email` mentions stay plain text).
// A token is only decorated when it resolves to a visible issue; unresolved
// tokens render as plain text. Clicking a pill navigates: plain click in
// read-only editors (comments, public views), Cmd/Ctrl+click while editing
// (so the caret still works, mirroring link behavior).

export interface IssueRefOptions {
  /** Resolve an identifier to display info, or null when unknown. Called on
   *  every decoration pass — must be cheap (a Map lookup). */
  getResolved: (identifier: string) => { title: string } | null
  /** Navigate to the referenced issue. */
  onOpen: (identifier: string) => void
}

function buildDecorations(
  doc: ProseMirrorNode,
  getResolved: IssueRefOptions[`getResolved`]
): Decoration[] {
  const decorations: Decoration[] = []
  doc.descendants((node, pos) => {
    if (node.type.name === `codeBlock`) return false
    if (!node.isText || !node.text) return undefined
    if (node.marks.some((mark) => mark.type.name === `code`)) return undefined

    const regExp = createIssueRefRegExp()
    let match: RegExpExecArray | null
    while ((match = regExp.exec(node.text)) !== null) {
      const identifier = match[1]
      const resolved = getResolved(identifier)
      if (!resolved) continue
      const from = pos + match.index
      decorations.push(
        Decoration.inline(from, from + match[0].length, {
          class: `issue-ref-pill`,
          "data-issue-ref": identifier,
          title: resolved.title,
        })
      )
    }
    return undefined
  })
  return decorations
}

export const IssueRefExtension = Extension.create<IssueRefOptions>({
  name: `issueRefPills`,

  addOptions() {
    return {
      getResolved: () => null,
      onOpen: () => {},
    }
  },

  addProseMirrorPlugins() {
    const { options } = this
    return [
      new Plugin({
        key: new PluginKey(`issueRefPills`),
        props: {
          decorations(state) {
            return DecorationSet.create(
              state.doc,
              buildDecorations(state.doc, options.getResolved)
            )
          },
          handleClick(view, _pos, event) {
            const target =
              event.target instanceof Element
                ? event.target.closest(`[data-issue-ref]`)
                : null
            if (!target) return false
            // While editing, plain click just places the caret (like links);
            // navigation needs a modifier.
            if (view.editable && !(event.metaKey || event.ctrlKey)) {
              return false
            }
            const identifier = target.getAttribute(`data-issue-ref`)
            if (!identifier) return false
            options.onOpen(identifier)
            return true
          },
        },
      }),
    ]
  },
})
