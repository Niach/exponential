import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react"
import type { User } from "@/db/schema"
import { Textarea } from "@/components/ui/textarea"
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
  applySkinTone,
  findEmojiByShortcode,
  matchEmojiToken,
  pushRecentEmoji,
  readSkinTone,
  searchEmoji,
  useEmojiData,
  type EmojiRecord,
} from "@/lib/emoji"

// Matches an in-progress mention `@query` at the caret (after start-of-text or
// whitespace). The query stops at whitespace so it won't swallow the rest.
const MENTION_AT_CARET = /(?:^|\s)@([a-zA-Z0-9._%+-]*)$/
// Same shape for an in-progress issue reference `#query` at the caret.
const ISSUE_REF_AT_CARET = /(?:^|\s)#([a-zA-Z0-9-]*)$/
// `:shortcode` (EXP-551) lives in lib/emoji.ts (`matchEmojiToken`) — shared
// with the TipTap editor's detector.

type AutocompleteMenu = {
  kind: `mention` | `issueRef` | `emoji`
  query: string
  start: number
  /** `emoji` only: the closing colon has been typed (`:tada:`). */
  closed?: boolean
}

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
   *  behind it and re-focuses the textarea. */
  insertText: (text: string) => void
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
  const issueRefs = useIssueRefs()
  const [menu, setMenu] = useState<AutocompleteMenu | null>(null)
  const [active, setActive] = useState(0)
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
  const issueCandidates =
    menu?.kind === `issueRef` && issueRefs
      ? issueRefs.search(menu.query, { limit: 6 })
      : []
  const emojiCandidates =
    menu?.kind === `emoji` && emojiData
      ? searchEmoji(emojiData, menu.query, 8)
      : []
  const candidateCount =
    menu?.kind === `mention`
      ? mentionCandidates.length
      : menu?.kind === `issueRef`
        ? issueCandidates.length
        : emojiCandidates.length

  const sync = (next: string, caret: number) => {
    onValueChange(next)
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
      setActive(0)
    } else if (issueRef) {
      setMenu({
        kind: `issueRef`,
        query: issueRef[1].toLowerCase(),
        start: caret - issueRef[1].length - 1,
      })
      setActive(0)
    } else if (emoji) {
      setMenu({
        kind: `emoji`,
        query: emoji.query.toLowerCase(),
        start: caret - emoji.length,
        closed: emoji.closed,
      })
      setActive(0)
    } else {
      setMenu(null)
    }
  }

  // Replaces [start, end) with `text`, restores focus and puts the caret
  // right behind the inserted text (after React has applied the new value).
  const splice = (start: number, end: number, text: string) => {
    const el = textareaRef.current
    const next = `${value.slice(0, start)}${text}${value.slice(end)}`
    const nextCaret = start + text.length
    onValueChange(next)
    setMenu(null)
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
    insertToken(applySkinTone(emoji, readSkinTone()), trailingSpace)
  }

  const insertActive = () => {
    if (menu?.kind === `mention` && mentionCandidates[active]) {
      insertMention(mentionCandidates[active])
    } else if (menu?.kind === `issueRef` && issueCandidates[active]) {
      insertIssueRef(issueCandidates[active])
    } else if (menu?.kind === `emoji` && emojiCandidates[active]) {
      insertEmoji(emojiCandidates[active], true)
    }
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
    insertText: (text: string) => {
      const el = textareaRef.current
      // selectionStart/End survive blur, so a picker that took focus still
      // knows where the caret was.
      const start = el?.selectionStart ?? value.length
      const end = el?.selectionEnd ?? start
      splice(start, end, text)
    },
  }))

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (menu && candidateCount > 0) {
      if (e.key === `ArrowDown`) {
        e.preventDefault()
        setActive((a) => (a + 1) % candidateCount)
        return
      }
      if (e.key === `ArrowUp`) {
        e.preventDefault()
        setActive((a) => (a - 1 + candidateCount) % candidateCount)
        return
      }
      // Plain Enter/Tab accept the highlighted candidate; a modified Enter
      // (Cmd/Ctrl+Enter = send in the comment composer) falls through to the
      // host — a menu must never swallow the send shortcut.
      if (
        (e.key === `Enter` || e.key === `Tab`) &&
        !e.metaKey &&
        !e.ctrlKey
      ) {
        e.preventDefault()
        insertActive()
        return
      }
      if (e.key === `Escape`) {
        e.preventDefault()
        setMenu(null)
        return
      }
    }
    onKeyDown?.(e)
  }

  return (
    <div className="relative flex-1">
      <Textarea
        {...props}
        ref={textareaRef}
        value={value}
        onChange={(e) => sync(e.target.value, e.target.selectionStart ?? 0)}
        onKeyDown={handleKeyDown}
      />
      {menu && candidateCount > 0 && (
        <div className="absolute bottom-full z-20 mb-1 w-72 overflow-hidden rounded-xl glass-panel">
          {menu.kind === `mention` &&
            mentionCandidates.map((u, i) => (
              <UserCandidateRow
                key={u.id}
                user={u}
                active={i === active}
                onSelect={() => insertMention(u)}
                onHover={() => setActive(i)}
              />
            ))}
          {menu.kind === `issueRef` &&
            issueCandidates.map((issue, i) => (
              <IssueCandidateRow
                key={issue.id}
                issue={issue}
                active={i === active}
                onSelect={() => insertIssueRef(issue)}
                onHover={() => setActive(i)}
              />
            ))}
          {menu.kind === `emoji` &&
            emojiCandidates.map((emoji, i) => (
              <EmojiCandidateRow
                key={emoji.u}
                emoji={emoji}
                unicode={applySkinTone(emoji, readSkinTone())}
                query={menu.query}
                active={i === active}
                onSelect={() => insertEmoji(emoji, true)}
                onHover={() => setActive(i)}
              />
            ))}
        </div>
      )}
    </div>
  )
})
