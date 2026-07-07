import { Extension } from "@tiptap/core"
import { Plugin, PluginKey } from "@tiptap/pm/state"
import { Decoration, DecorationSet } from "@tiptap/pm/view"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"
import { createMentionRegExp } from "@/lib/mention-refs"

// Renders `@email` mentions as name pills via decorations — the document text
// stays the plain token, so the GFM markdown round-trip is untouched (exactly
// like the `#IDENTIFIER` pills in lib/issue-ref-extension.ts). A token is only
// decorated when the email resolves to a workspace member the viewer can see
// (the users shape already excludes public-workspace co-members, so
// anonymization holds by construction); unresolved tokens render as plain
// text.
//
// Read-only editors (comment display, public views) replace the raw email with
// the member's name — `@Ada Lovelace` — via a widget over the hidden source
// text. Editable editors keep the `@email` text visible (hiding characters
// under an active caret makes editing hazardous) and just style it as a pill
// with the name as tooltip.

export interface MentionPillOptions {
  /** Resolve a mention email to display info, or null when unknown. Called on
   *  every decoration pass — must be cheap (a Map lookup). */
  getResolved: (email: string) => { name: string } | null
}

function buildDecorations(
  doc: ProseMirrorNode,
  editable: boolean,
  getResolved: MentionPillOptions[`getResolved`]
): Decoration[] {
  const decorations: Decoration[] = []
  doc.descendants((node, pos) => {
    if (node.type.name === `codeBlock`) return false
    if (!node.isText || !node.text) return undefined
    if (node.marks.some((mark) => mark.type.name === `code`)) return undefined

    const regExp = createMentionRegExp()
    let match: RegExpExecArray | null
    while ((match = regExp.exec(node.text)) !== null) {
      const email = match[1]
      const resolved = getResolved(email)
      if (!resolved) continue
      const from = pos + match.index
      const to = from + match[0].length
      if (editable) {
        decorations.push(
          Decoration.inline(from, to, {
            class: `mention-pill`,
            "data-mention-email": email,
            title: resolved.name,
          })
        )
      } else {
        decorations.push(
          Decoration.inline(from, to, { class: `mention-pill-source` }),
          Decoration.widget(
            from,
            () => {
              const el = document.createElement(`span`)
              el.className = `mention-pill`
              el.textContent = `@${resolved.name}`
              el.title = email
              return el
            },
            { key: `mention-${from}-${email}-${resolved.name}`, side: 1 }
          )
        )
      }
    }
    return undefined
  })
  return decorations
}

export const MentionPillExtension = Extension.create<MentionPillOptions>({
  name: `mentionPills`,

  addOptions() {
    return {
      getResolved: () => null,
    }
  },

  addProseMirrorPlugins() {
    const { editor, options } = this
    return [
      new Plugin({
        key: new PluginKey(`mentionPills`),
        props: {
          decorations(state) {
            return DecorationSet.create(
              state.doc,
              buildDecorations(
                state.doc,
                editor.isEditable,
                options.getResolved
              )
            )
          },
        },
      }),
    ]
  },
})
