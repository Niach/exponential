import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
} from "react"
import type { User } from "@/db/schema"
import { Textarea, TypeaheadMenu, useTypeahead } from "@exp/ui"
import {
  EmojiCandidateRow,
  IssueCandidateRow,
  UserCandidateRow,
} from "@/components/autocomplete-rows"
import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"
import {
  findEmojiByShortcode,
  matchEmojiToken,
  pushRecentEmoji,
  searchEmoji,
  useEmojiData,
  type EmojiRecord,
} from "@/lib/emoji"
import { ISSUE_REF_AT_CARET } from "@/lib/issue-refs"
import { useIssueSearchResults } from "@/hooks/use-issue-search-results"
import { MENTION_AT_CARET } from "@/lib/mention-refs"

// `:shortcode` (EXP-551) lives in lib/emoji.ts (`matchEmojiToken`) — shared
// with the TipTap editor's detector.

const NO_ROWS: ResolvedIssueRef[] = []

type AutocompleteMenu = {
  kind: `mention` | `issueRef` | `emoji`
  query: string
  start: number
  /** `emoji` only: the closing colon has been typed (`:tada:`). */
  closed?: boolean
}

/** The open menu's rows as ONE list — `useTypeahead` (EXP-941) owns the active
 *  index, and only one kind of candidate is ever offered at a time. */
type Candidate =
  | { kind: `mention`; user: User }
  | { kind: `issueRef`; issue: ResolvedIssueRef }
  | { kind: `emoji`; emoji: EmojiRecord }

interface MentionTextareaProps extends Omit<
  React.ComponentProps<typeof Textarea>,
  `value` | `onChange`
> {
  value: string
  onValueChange: (next: string) => void
  // Team members to offer; agents are filtered out (you mention people).
  users: User[]
}

/** Imperative surface for hosts that insert text from outside the textarea
 *  (the comment composer's emoji picker, EXP-551). */
export interface MentionTextareaHandle {
  /** Inserts `text` at the caret (replacing any selection), moves the caret
   *  behind it, or `caretOffset` characters into it when given (EXP-827: a
   *  suggestion like "Start a session for # on my other machine" lands the
   *  caret right after its `#`, so the issue-ref menu opens at once),
   *  behind it and re-focuses the textarea. */
  insertText: (text: string, caretOffset?: number) => void
  /** The character immediately before the caret, or undefined at the very
   *  start. The `#` button needs it: the issue-ref autocomplete only triggers
   *  at a token start, so a `#` typed straight after a word needs a space in
   *  front of it (EXP-568). */
  charBeforeCaret: () => string | undefined
  /** The caret offset into the current value. A host that rewrites the WHOLE
   *  draft around the caret — the steer composer's `[Image #N]` markers
   *  (EXP-698) — needs the position before it computes the new text. */
  caret: () => number
  /** Puts the caret at `position` once React has applied the value the host
   *  just committed. Focus is restored only if the field still had it when
   *  `caret()` was called — an image attached from the picker must not pop
   *  the phone keyboard. */
  setCaret: (position: number) => void
}

// A Textarea with @-mention, #-issue-reference and :emoji autocomplete.
// Selecting a member inserts the canonical `@<email>` form the server resolves
// (lib/integrations/mentions.ts); selecting an issue inserts the `#IDENTIFIER`
// token the clients render as a pill (lib/issue-refs.ts); selecting an emoji
// inserts its unicode (never the shortcode). Issue suggestions come from the
// team IssueRefProvider (absent outside a team → the # trigger is simply
// inert).
export const MentionTextarea = forwardRef<
  MentionTextareaHandle,
  MentionTextareaProps
>(function MentionTextarea(
  { value, onValueChange, users, onKeyDown, ...props },
  ref
) {
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  // Latched by `caret()`, spent by `setCaret()` (EXP-698).
  const caretReadWhileFocusedRef = useRef(false)
  const issueRefs = useIssueRefs()
  const [menu, setMenu] = useState<AutocompleteMenu | null>(null)
  const emojiData = useEmojiData(menu?.kind === `emoji`)

  const people = users
  const mentionCandidates =
    menu?.kind === `mention`
      ? people
          .filter(
            (u) =>
              u.name.toLowerCase().includes(menu.query) ||
              u.email.toLowerCase().includes(menu.query)
          )
          .slice(0, 6)
      : []
  // EXP-892: the shared engine — local ranking now, the server's full-text
  // hits spliced in behind once they answer (the same list every `#` picker
  // shows on every client).
  const issueMenuOpen = menu?.kind === `issueRef` && issueRefs !== null
  const { results: issueCandidates } = useIssueSearchResults({
    teamId: issueRefs?.teamId,
    query: issueMenuOpen ? menu.query : ``,
    rows: issueMenuOpen ? issueRefs.rows : NO_ROWS,
    limit: 6,
    resolveHit: (hit) => issueRefs?.fromHit(hit) ?? null,
    server: issueMenuOpen,
  })
  const emojiCandidates =
    menu?.kind === `emoji` && emojiData
      ? searchEmoji(emojiData, menu.query, 8)
      : []
  const candidates: Candidate[] =
    menu?.kind === `mention`
      ? mentionCandidates.map((user) => ({ kind: `mention`, user }))
      : menu?.kind === `issueRef`
        ? issueCandidates.map((issue) => ({ kind: `issueRef`, issue }))
        : menu?.kind === `emoji`
          ? emojiCandidates.map((emoji) => ({ kind: `emoji`, emoji }))
          : []
  const candidateCount = candidates.length

  // EXP-941: the arrows, the wrap, the plain-Enter/Tab accept, the Escape and
  // the "a modified Enter is the composer's send, not ours" rule all live in
  // the shared hook — the `/` menu and the TipTap editor run the same one.
  const typeahead = useTypeahead<Candidate>({
    items: candidates,
    onAccept: (candidate) => {
      if (candidate.kind === `mention`) {
        insertMention(candidate.user)
      } else if (candidate.kind === `issueRef`) {
        insertIssueRef(candidate.issue)
      } else {
        insertEmoji(candidate.emoji, true)
      }
    },
    onDismiss: () => setMenu(null),
    // A new token (or a new kind of token) starts at the top row again.
    resetKey: menu ? `${menu.kind}:${menu.start}:${menu.query}` : null,
  })
  const active = typeahead.active

  // EXP-946: the menu opens ABOVE the field — a composer normally sits at the
  // bottom of the screen. A field near the TOP of the window has no room
  // there, and the menu used to run straight off it, so measure both sides
  // and flip; whichever side wins is capped to the room it has and scrolls.
  const wrapRef = useRef<HTMLDivElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const [above, setAbove] = useState(true)
  const [menuMaxHeight, setMenuMaxHeight] = useState<number | null>(null)
  useLayoutEffect(() => {
    const wrap = wrapRef.current
    const el = menuRef.current
    if (!wrap || !el) return
    const rect = wrap.getBoundingClientRect()
    const gutter = 12
    const roomAbove = Math.max(0, rect.top - gutter)
    const roomBelow = Math.max(0, window.innerHeight - rect.bottom - gutter)
    // `scrollHeight` is the FULL list even while a cap is on it, so the
    // choice never depends on the cap the last open left behind.
    const fitsAbove = el.scrollHeight <= roomAbove
    const next = fitsAbove || roomAbove >= roomBelow
    setAbove(next)
    setMenuMaxHeight(Math.round(next ? roomAbove : roomBelow))
  }, [menu, candidateCount])

  const sync = (next: string, caret: number) => {
    onValueChange(next)
    detect(next, caret)
  }

  /** Which autocomplete (if any) the text before `caret` is in the middle
   *  of. Runs on every keystroke, and (EXP-790) after a programmatic insert,
   *  so a chip that drops a trailing `#` opens the issue picker at once. */
  const detect = (next: string, caret: number) => {
    const before = next.slice(0, caret)
    const mention = before.match(MENTION_AT_CARET)
    const issueRef = issueRefs ? before.match(ISSUE_REF_AT_CARET) : null
    const emoji = matchEmojiToken(before)
    if (mention) {
      setMenu({
        kind: `mention`,
        query: mention[1].toLowerCase(),
        start: caret - mention[1].length - 1,
      })
    } else if (issueRef) {
      setMenu({
        kind: `issueRef`,
        query: issueRef[1].toLowerCase(),
        start: caret - issueRef[1].length - 1,
      })
    } else if (emoji) {
      setMenu({
        kind: `emoji`,
        query: emoji.query.toLowerCase(),
        start: caret - emoji.length,
        closed: emoji.closed,
      })
    } else {
      setMenu(null)
    }
  }

  // Replaces [start, end) with `text`, restores focus and puts the caret
  // right behind the inserted text (after React has applied the new value).
  const splice = (
    start: number,
    end: number,
    text: string,
    caretOffset: number = text.length
  ) => {
    const el = textareaRef.current
    const next = `${value.slice(0, start)}${text}${value.slice(end)}`
    const nextCaret = start + Math.max(0, Math.min(caretOffset, text.length))
    onValueChange(next)
    detect(next, nextCaret)
    requestAnimationFrame(() => {
      if (el) {
        el.focus()
        el.setSelectionRange(nextCaret, nextCaret)
      }
    })
  }

  // Replace the in-progress `@query`/`#query`/`:query` token with the
  // canonical form.
  const insertToken = (token: string, trailingSpace = true) => {
    if (!menu) return
    const caret = textareaRef.current?.selectionStart ?? value.length
    splice(menu.start, caret, trailingSpace ? `${token} ` : token)
  }

  const insertMention = (user: User) => insertToken(`@${user.email}`)
  const insertIssueRef = (issue: ResolvedIssueRef) =>
    insertToken(`#${issue.identifier}`)
  const insertEmoji = (emoji: EmojiRecord, trailingSpace: boolean) => {
    pushRecentEmoji(emoji.u)
    insertToken(emoji.u, trailingSpace)
  }

  // `:tada:` typed in full commits the exact shortcode at once (no trailing
  // space) — see markdown-editor.tsx for the same rule in the TipTap editor.
  const autoCommitRef = useRef<string | null>(null)
  useEffect(() => {
    if (!menu || menu.kind !== `emoji`) {
      autoCommitRef.current = null
      return
    }
    if (!menu.closed || !emojiData) return
    const key = `${menu.start}:${menu.query}`
    if (autoCommitRef.current === key) return
    const exact = findEmojiByShortcode(emojiData, menu.query)
    if (!exact) return
    autoCommitRef.current = key
    insertEmoji(exact, false)
  }, [menu, emojiData])

  useImperativeHandle(ref, () => ({
    insertText: (text: string, caretOffset?: number) => {
      const el = textareaRef.current
      // selectionStart/End survive blur, so a picker that took focus still
      // knows where the caret was.
      const start = el?.selectionStart ?? value.length
      const end = el?.selectionEnd ?? start
      splice(start, end, text, caretOffset)
    },
    charBeforeCaret: () => {
      const el = textareaRef.current
      const start = el?.selectionStart ?? value.length
      return start > 0 ? value.slice(start - 1, start) : undefined
    },
    caret: () => {
      const el = textareaRef.current
      // Whether the field HAD focus decides whether `setCaret` may take it
      // back — see below.
      caretReadWhileFocusedRef.current =
        el != null && document.activeElement === el
      return el?.selectionStart ?? value.length
    },
    setCaret: (position: number) => {
      const el = textareaRef.current
      const refocus = caretReadWhileFocusedRef.current
      requestAnimationFrame(() => {
        if (!el) return
        // Focus is only RESTORED, never taken: attaching an image from the
        // picker or a drop happens with the field blurred, and focusing it
        // there throws the phone's keyboard up over the composer.
        if (refocus) el.focus()
        el.setSelectionRange(position, position)
      })
    },
  }))

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Anything the open menu consumed (arrows, a plain Enter/Tab, Escape) the
    // host never sees; a modified Enter (Cmd/Ctrl+Enter = send in the comment
    // composer) is never consumed, so the send shortcut always gets through.
    if (typeahead.handleKeyDown(e)) return
    onKeyDown?.(e)
  }

  return (
    <div ref={wrapRef} className="relative flex-1">
      <Textarea
        {...props}
        ref={textareaRef}
        value={value}
        onChange={(e) => sync(e.target.value, e.target.selectionStart ?? 0)}
        onKeyDown={handleKeyDown}
      />
      {menu && candidateCount > 0 && (
        <TypeaheadMenu
          ref={menuRef}
          placement={above ? `above` : `below`}
          maxHeight={menuMaxHeight ?? undefined}
        >
          {menu.kind === `mention` &&
            mentionCandidates.map((u, i) => (
              <UserCandidateRow
                key={u.id}
                user={u}
                active={i === active}
                onSelect={() => insertMention(u)}
                onHover={() => typeahead.setActive(i)}
              />
            ))}
          {menu.kind === `issueRef` &&
            issueCandidates.map((issue, i) => (
              <IssueCandidateRow
                key={issue.id}
                issue={issue}
                active={i === active}
                onSelect={() => insertIssueRef(issue)}
                onHover={() => typeahead.setActive(i)}
              />
            ))}
          {menu.kind === `emoji` &&
            emojiCandidates.map((emoji, i) => (
              <EmojiCandidateRow
                key={emoji.u}
                emoji={emoji}
                unicode={emoji.u}
                query={menu.query}
                active={i === active}
                onSelect={() => insertEmoji(emoji, true)}
                onHover={() => typeahead.setActive(i)}
              />
            ))}
        </TypeaheadMenu>
      )}
    </div>
  )
})
