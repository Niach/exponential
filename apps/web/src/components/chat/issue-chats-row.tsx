import { CHATS_COPY } from "@exp/domain-contract/chats"
import { useMemo } from "react"
import { Link } from "@tanstack/react-router"
import { AgentBrandMark, AgentWorkingMark, Pill } from "@exp/ui"
import { useSessionRow, useSessionTitles, useChatsOfIssue } from "@/hooks/use-session-tree"
import type { SessionRow } from "@/lib/session-tree"

// EXP-1284 / EXP-1293: the issue page NAMES the chats it sits in: a
// `Chats` row under the title, one chip per chat (a ROOT session linked
// through `session_issues`): the agent's mark (working while it works) and
// the chat's title (`sessionTitle`, `useSessionTitles`), opening the chat page. Absent while
// the issue sits in none.

/** Chat 58cb32df: the product says CHATS (chats.json `copy.title`). */
export const ISSUE_CHATS_LABEL = CHATS_COPY.title
/** The draft page's word before its chat chip. */
export const DRAFT_CHAT_LABEL = `In`

/** One chat as a chip: the agent's mark and the title, opening it. */
function ChatChip({
  root,
  title,
  teamSlug,
}: {
  root: SessionRow
  title: string
  teamSlug: string
}) {
  const working = root.status !== `ended` && root.busy
  return (
    <Pill
      asChild
      mode="action"
      className="max-w-[18rem] min-w-0 text-foreground"
      leading={
        working ? (
          <AgentWorkingMark agent={root.cli} className="size-3.5" />
        ) : (
          <AgentBrandMark agent={root.cli} className="size-3.5" />
        )
      }
    >
      <Link
        to="/t/$teamSlug/sessions/$sessionId"
        params={{ teamSlug, sessionId: root.id }}
        data-testid={`issue-chat-chip-${root.id}`}
      >
        <span className="min-w-0 truncate">{title}</span>
      </Link>
    </Pill>
  )
}

function ChatChipRow({
  label,
  roots,
  teamSlug,
  testId,
  className,
}: {
  label: string
  roots: readonly SessionRow[]
  teamSlug: string
  testId: string
  className?: string
}) {
  // EXP-1316: no chip until every title settled (`useSessionTitles`).
  const { titles, ready } = useSessionTitles(roots)
  if (roots.length === 0 || !ready) return null
  return (
    <div
      data-testid={testId}
      className={`flex min-w-0 flex-wrap items-center gap-1.5 text-sm ${className ?? ``}`}
    >
      <span className="mr-1 shrink-0 text-xs text-muted-foreground">{label}</span>
      {roots.map((root) => (
        <ChatChip
          key={root.id}
          root={root}
          title={titles.get(root.id) ?? ``}
          teamSlug={teamSlug}
        />
      ))}
    </div>
  )
}

/** EXP-1284: a "New issue in this chat" draft says where it files: `In
 *  <chat>` under the title. Absent for a plain draft (or a chat not
 *  synced / gone). */
export function DraftChatRow({
  chatId,
  teamSlug,
  className,
}: {
  chatId: string | null
  /** Kept for the draft page's call; the root row carries its team. */
  teamId?: string
  teamSlug: string
  className?: string
}) {
  const { row } = useSessionRow(chatId)
  const roots = useMemo(
    () => (row && row.parentId === null ? [row] : []),
    [row]
  )
  return (
    <ChatChipRow
      label={DRAFT_CHAT_LABEL}
      roots={roots}
      teamSlug={teamSlug}
      testId="draft-chat-row"
      className={className}
    />
  )
}

export function IssueChatsRow({
  issueId,
  teamSlug,
  className,
}: {
  issueId: string
  /** Kept for the callers; the links carry their team. */
  teamId?: string
  teamSlug: string
  className?: string
}) {
  const roots = useChatsOfIssue(issueId)
  // Chats only: an agent's conversation is not a chat the issue sits in.
  const chats = useMemo(
    () => roots.filter((root) => root.parentId === null && !root.agentId && !root.appId),
    [roots]
  )
  return (
    <ChatChipRow
      label={ISSUE_CHATS_LABEL}
      roots={chats}
      teamSlug={teamSlug}
      testId="issue-chats-row"
      className={className}
    />
  )
}
