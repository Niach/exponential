import { useRef, useState } from "react"
import type { User } from "@/db/schema"
import { Textarea } from "@/components/ui/textarea"
import {
  IssueCandidateRow,
  UserCandidateRow,
} from "@/components/autocomplete-rows"
import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"

// Matches an in-progress mention `@query` at the caret (after start-of-text or
// whitespace). The query stops at whitespace so it won't swallow the rest.
const MENTION_AT_CARET = /(?:^|\s)@([a-zA-Z0-9._%+-]*)$/
// Same shape for an in-progress issue reference `#query` at the caret.
const ISSUE_REF_AT_CARET = /(?:^|\s)#([a-zA-Z0-9-]*)$/

type AutocompleteMenu = {
  kind: `mention` | `issueRef`
  query: string
  start: number
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

// A Textarea with @-mention and #-issue-reference autocomplete. Selecting a
// member inserts the canonical `@<email>` form the server resolves
// (lib/integrations/mentions.ts); selecting an issue inserts the `#IDENTIFIER`
// token the clients render as a pill (lib/issue-refs.ts). Issue suggestions
// come from the team IssueRefProvider (absent outside a team → the
// # trigger is simply inert).
export function MentionTextarea({
  value,
  onValueChange,
  users,
  onKeyDown,
  ...props
}: MentionTextareaProps) {
  const ref = useRef<HTMLTextAreaElement>(null)
  const issueRefs = useIssueRefs()
  const [menu, setMenu] = useState<AutocompleteMenu | null>(null)
  const [active, setActive] = useState(0)

  const people = users.filter((u) => !u.isAgent)
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
  const candidateCount =
    menu?.kind === `mention` ? mentionCandidates.length : issueCandidates.length

  const sync = (next: string, caret: number) => {
    onValueChange(next)
    const before = next.slice(0, caret)
    const mention = before.match(MENTION_AT_CARET)
    const issueRef = issueRefs ? before.match(ISSUE_REF_AT_CARET) : null
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
    } else {
      setMenu(null)
    }
  }

  // Replace the in-progress `@query`/`#query` token with the canonical form.
  const insertToken = (token: string) => {
    if (!menu) return
    const el = ref.current
    const caret = el?.selectionStart ?? value.length
    const next = `${value.slice(0, menu.start)}${token} ${value.slice(caret)}`
    const nextCaret = menu.start + token.length + 1
    onValueChange(next)
    setMenu(null)
    requestAnimationFrame(() => {
      if (el) {
        el.focus()
        el.setSelectionRange(nextCaret, nextCaret)
      }
    })
  }

  const insertMention = (user: User) => insertToken(`@${user.email}`)
  const insertIssueRef = (issue: ResolvedIssueRef) =>
    insertToken(`#${issue.identifier}`)

  const insertActive = () => {
    if (menu?.kind === `mention` && mentionCandidates[active]) {
      insertMention(mentionCandidates[active])
    } else if (menu?.kind === `issueRef` && issueCandidates[active]) {
      insertIssueRef(issueCandidates[active])
    }
  }

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
      if (e.key === `Enter` || e.key === `Tab`) {
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
        ref={ref}
        value={value}
        onChange={(e) => sync(e.target.value, e.target.selectionStart ?? 0)}
        onKeyDown={handleKeyDown}
      />
      {menu && candidateCount > 0 && (
        <div className="absolute bottom-full z-20 mb-1 w-72 overflow-hidden rounded-md border bg-popover shadow-md">
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
        </div>
      )}
    </div>
  )
}
