import { Extension } from "@tiptap/core"
import { Plugin, PluginKey } from "@tiptap/pm/state"
import { Decoration, DecorationSet } from "@tiptap/pm/view"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"
import type { IconName } from "@exp/icons"
import { createIssueRefRegExp } from "@/lib/issue-refs"
import { statusIconDataUri } from "@/lib/status-icon-svg"

// Renders `#IDENTIFIER` issue references as clickable pills via inline
// decorations — the document text stays the plain token, so the GFM markdown
// round-trip is untouched (mirrors how `@email` mentions stay plain text).
// A token is only decorated when it resolves to a visible issue; unresolved
// tokens render as plain text. The chip shows the identifier plus the issue
// title (the title rides a data attribute rendered by CSS ::after, so the
// editable text stays exactly the token), and clicking it navigates — plain
// click in read-only AND editable editors (EXP-307; the caret can still be
// placed via the surrounding text or the keyboard).

export interface IssueRefOptions {
  /** Resolve an identifier to display info, or null when unknown. Called on
   *  every decoration pass — must be cheap (a Map lookup). */
  getResolved: (identifier: string) => {
    title: string
    statusIcon: IconName
    statusColor: string
  } | null
  /** Navigate to the referenced issue. */
  onOpen: (identifier: string) => void
}

/** Keep chips readable — the full title stays available as the tooltip. */
const MAX_CHIP_TITLE_LENGTH = 60

function chipTitle(title: string): string {
  const trimmed = title.trim()
  return trimmed.length > MAX_CHIP_TITLE_LENGTH
    ? `${trimmed.slice(0, MAX_CHIP_TITLE_LENGTH - 1).trimEnd()}…`
    : trimmed
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
          "data-issue-title": chipTitle(resolved.title),
          title: resolved.title,
          // EXP-423: the status glyph is a `::before` mask on the pill, fed
          // its shape and tint from here. Nothing about the document text
          // changes — same guarantee as the `::after` title.
          style: `--issue-ref-status-color: ${resolved.statusColor}; --issue-ref-status-icon: ${statusIconDataUri(resolved.statusIcon)}`,
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
          handleClick(_view, _pos, event) {
            const target =
              event.target instanceof Element
                ? event.target.closest(`[data-issue-ref]`)
                : null
            if (!target) return false
            // EXP-307: a chip is a chip — plain click navigates even while
            // editing (clicking a pill used to just drop the caret into the
            // token, which popped the #-autocomplete instead of opening the
            // issue).
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
