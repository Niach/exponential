import { useMemo } from "react"
import { SESSION_SCREEN_TEST_IDS } from "@exp/domain-contract/session-screen"
import {
  ChatSessionTree,
  conceptIcon,
  EmptyState,
  ListRow,
  menuProps,
  StatusGlyph,
  type ChatTreeRow,
  type StatusGlyphProps,
} from "@exp/ui"
import { ChatResults } from "@/components/chat/chat-results"
import type { ResultsTabState } from "@/hooks/use-results-tab"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { sendSessionMessage } from "@/lib/enet/send-sealed"
import {
  sessionCard,
  sessionTabMarkState,
  toTreeSession,
  treeOrder,
  type PullRequestRow,
  type SessionRow,
} from "@/lib/session-tree"
import { cn } from "@/lib/utils"

// #EXP-1732 (session-screen.json BODIES): the tab bodies every kind shares:
// Issues (THE issue rows led by the `+`), Sessions (THE tree rows of every
// session under the root) and Results (results-tab.json's body). An app's
// editor renders Sessions and Results over its editor session's tree.

const IssuesIcon = conceptIcon(`nav-issues`)
const SessionsIcon = conceptIcon(`session-tree`)

/** The issues tab's context menu kind (`menuProps`): Unlink rides it. */
export const SESSION_ISSUE_MENU = `chat-issue`

export interface SessionIssueEntry {
  issueId: string
  identifier: string
  title: string | null
  /** null = the issue row is not synced: no glyph (never Backlog). */
  status: StatusGlyphProps | null
  /** A sub-issue (indented under its parent). */
  nested: boolean
}

/** Issues: the `+` (THE Menu: Link an issue ›, New issue), then the tree's
 *  issues, nested ones indented under their parent; a row opens it in place. */
export function SessionIssuesTab({
  issues,
  activeId,
  onOpen,
}: {
  issues: readonly SessionIssueEntry[]
  activeId: string | null
  onOpen: (issueId: string) => void
}) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain" data-testid={SESSION_SCREEN_TEST_IDS.issues}>
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-1 px-3 pt-3 pb-8 md:px-4">
        {issues.length === 0 ? (
          <EmptyState icon={IssuesIcon} title="No issues" size="list" />
        ) : (
          issues.map((issue) => (
            <ListRow
              key={issue.issueId}
              interactive
              active={issue.issueId === activeId}
              data-testid="session-issue-row"
              className={cn(`gap-2 text-sm`, issue.nested && `pl-8`)}
              onClick={() => onOpen(issue.issueId)}
              {...menuProps(SESSION_ISSUE_MENU, issue.issueId)}
            >
              {issue.status ? (
                <StatusGlyph {...issue.status} className="size-3.5 shrink-0" />
              ) : (
                <span aria-hidden className="size-3.5 shrink-0" />
              )}
              <span className="shrink-0 font-mono text-xs text-muted-foreground">{issue.identifier}</span>
              <span className="min-w-0 flex-1 truncate">{issue.title ?? ``}</span>
            </ListRow>
          ))
        )}
      </div>
    </div>
  )
}

/** THE tree rows of every session UNDER the root (the root itself never). */
export function useSessionTreeRows(
  rows: readonly SessionRow[],
  titles: ReadonlyMap<string, string>,
  rootId: string | null
): ChatTreeRow[] {
  return useMemo(() => {
    if (!rootId) return []
    const nowIso = new Date().toISOString()
    const byId = new Map(rows.map((row) => [row.id, row]))
    return treeOrder(rows.map(toTreeSession))
      .filter((session) => session.id !== rootId)
      .map((session) => {
        const card = sessionCard(session, titles.get(session.id) ?? ``, nowIso)
        return {
          sessionId: session.id,
          parentId: session.parentId === rootId ? null : session.parentId,
          name: card.name,
          agent: byId.get(session.id)?.cli ?? null,
          markState: sessionTabMarkState(session),
          stateWord: card.kindWord,
          stateTone: card.tone,
        }
      })
  }, [rows, titles, rootId])
}

/** Sessions: THE tree rows (SessionRow small + TreeGuides, markState); a
 *  row opens that session in place. */
export function SessionSessionsTab({
  rows,
  activeId,
  onOpen,
}: {
  rows: readonly ChatTreeRow[]
  activeId: string | null
  onOpen: (sessionId: string) => void
}) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain" data-testid={SESSION_SCREEN_TEST_IDS.sessions}>
      <div className="mx-auto w-full max-w-3xl px-3 pt-3 pb-8 md:px-4">
        {rows.length === 0 ? (
          <EmptyState icon={SessionsIcon} title="No sessions" size="list" />
        ) : (
          <ChatSessionTree rows={rows} activeId={activeId} onOpen={onOpen} />
        )}
      </div>
    </div>
  )
}

/** Results: today's Results body (results-tab.json). */
export function SessionResultsTab({
  teamId,
  teamSlug,
  results,
  pullRequest,
  canMerge,
  review,
  file,
  onReview,
}: {
  teamId: string
  teamSlug: string
  results: ResultsTabState
  pullRequest: PullRequestRow | null
  canMerge: boolean
  review: boolean
  file: string | null
  onReview: (open: boolean, file?: string | null) => void
}) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain" data-testid="chat-results-scroll">
      <div className="mx-auto w-full max-w-3xl px-4 pt-6 pb-10 md:px-5">
        <ChatResults
          tab={results}
          teamSlug={teamSlug}
          pullRequest={pullRequest}
          canMerge={canMerge}
          review={review}
          file={file}
          onReview={onReview}
          onSurfaceReply={
            canMerge
              ? (result, text) =>
                  void sendSessionMessage({ teamId, sessionId: result.sessionId, text }, SKIP_ERROR_TOAST).catch(
                    (error) => failedAction(`send`, `the message`, error)
                  )
              : undefined
          }
        />
      </div>
    </div>
  )
}
