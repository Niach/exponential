import { Extension } from "@tiptap/core"
import { Plugin, PluginKey } from "@tiptap/pm/state"
import type { EditorState } from "@tiptap/pm/state"
import { matchEmojiToken } from "@/lib/emoji"

// Caret-anchored `@mention` / `#issueRef` / `:emoji` autocomplete for the
// TipTap editor.
// The extension only *detects* an in-progress token at the caret and reports
// it — candidate lookup, menu rendering, and the final insertion live in
// React (markdown-editor.tsx), mirroring how MentionTextarea drives the same
// UX for the plain-textarea comment composer. Selecting a candidate inserts
// the plain interchange text (`@<email>` / `#<IDENTIFIER>`) — never a custom
// node — so the GFM markdown round-trip is untouched. `:shortcode` picks
// insert the emoji's unicode (EXP-551), likewise plain text.

export type EditorAutocompleteKind = `mention` | `issueRef` | `emoji`

export interface EditorAutocompleteActive {
  kind: EditorAutocompleteKind
  /** The partial token typed after the trigger char (not yet lowercased). */
  query: string
  /** Doc position of the trigger character (`@` / `#`). */
  from: number
  /** Caret position (end of the in-progress token). */
  to: number
  /**
   * `emoji` only: the user has typed the closing colon (`:tada:`). The token
   * still spans [from, to) including that colon; an exact shortcode match
   * auto-commits.
   */
  closed?: boolean
}

export interface EditorAutocompleteOptions {
  /** Fired when the in-progress token at the caret appears/changes/vanishes. */
  onStateChange: (active: EditorAutocompleteActive | null) => void
  /** Keyboard hook while the editor is focused; return true to consume the
   *  event (menu navigation). */
  onKeyDown: (event: KeyboardEvent) => boolean
}

// Same token shapes as the comment composer (mention-textarea.tsx): the
// trigger must follow start-of-text or whitespace, and the query stops at the
// caret so it never swallows trailing text.
const MENTION_AT_CARET = /(?:^|\s)@([a-zA-Z0-9._%+-]*)$/
const ISSUE_REF_AT_CARET = /(?:^|\s)#([a-zA-Z0-9-]*)$/

export function findAutocompleteAtCaret(
  state: EditorState
): EditorAutocompleteActive | null {
  const { $from, empty } = state.selection
  if (!empty || !$from.parent.isTextblock) return null
  if ($from.parent.type.name === `codeBlock`) return null
  const marks = state.storedMarks ?? $from.marks()
  if (marks.some((mark) => mark.type.name === `code`)) return null

  // Leaf nodes (images) become an object-replacement char so a token can't
  // span across them — and so string offsets stay aligned with doc positions
  // (every position before the caret maps to exactly one char).
  const textBefore = $from.parent.textBetween(
    0,
    $from.parentOffset,
    undefined,
    `￼`
  )
  const mention = textBefore.match(MENTION_AT_CARET)
  if (mention) {
    return {
      kind: `mention`,
      query: mention[1],
      from: $from.pos - mention[1].length - 1,
      to: $from.pos,
    }
  }
  const issueRef = textBefore.match(ISSUE_REF_AT_CARET)
  if (issueRef) {
    return {
      kind: `issueRef`,
      query: issueRef[1],
      from: $from.pos - issueRef[1].length - 1,
      to: $from.pos,
    }
  }
  const emoji = matchEmojiToken(textBefore)
  if (emoji) {
    return {
      kind: `emoji`,
      query: emoji.query,
      from: $from.pos - emoji.length,
      to: $from.pos,
      closed: emoji.closed,
    }
  }
  return null
}

function sameActive(
  a: EditorAutocompleteActive | null,
  b: EditorAutocompleteActive | null
) {
  if (a === null || b === null) return a === b
  return (
    a.kind === b.kind &&
    a.query === b.query &&
    a.from === b.from &&
    a.to === b.to &&
    Boolean(a.closed) === Boolean(b.closed)
  )
}

export const EditorAutocompleteExtension =
  Extension.create<EditorAutocompleteOptions>({
    name: `editorAutocomplete`,

    addOptions() {
      return {
        onStateChange: () => {},
        onKeyDown: () => false,
      }
    },

    addProseMirrorPlugins() {
      const { options } = this
      let last: EditorAutocompleteActive | null = null
      const emit = (next: EditorAutocompleteActive | null) => {
        if (sameActive(last, next)) return
        last = next
        options.onStateChange(next)
      }
      return [
        new Plugin({
          key: new PluginKey(`editorAutocomplete`),
          view: () => ({
            update: (view, prevState) => {
              if (!view.editable) {
                emit(null)
                return
              }
              const next = findAutocompleteAtCaret(view.state)
              // Only typing may OPEN the menu. A pure caret move (clicking
              // right after an existing `#EXP-42` token, arrowing onto it)
              // matches the same at-caret regex, and used to pop the picker
              // on every click near a pill (EXP-307). An already-open menu
              // still tracks/dismisses on caret moves.
              const docChanged = !view.state.doc.eq(prevState.doc)
              if (next && last === null && !docChanged) return
              emit(next)
            },
            destroy: () => {
              emit(null)
            },
          }),
          props: {
            handleKeyDown: (_view, event) => options.onKeyDown(event),
            handleDOMEvents: {
              blur: () => {
                emit(null)
                return false
              },
            },
          },
        }),
      ]
    },
  })
