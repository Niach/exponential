import { useCallback, useEffect, useMemo, useState } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { GitBranch, GitPullRequest, Loader2, RotateCw } from "lucide-react"
import type { CodingSession, Issue, User } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"
import { codingSessionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import { DiffView, FileDiffList, type PullFile } from "@/components/diff-view"
import { IssueSteerPanel } from "@/components/agent-session"

// Issue "Changes" tab (masterplan §5.4 / §4.8 tiers 2–4). Web does no git ops
// (L18); remote visibility is capability-tiered and resolves to whichever tier
// has data:
//   2. PR exists            → PR diff (issues.prFiles, rendered as today)
//   3. branch pushed, no PR  → repositories.branchDiff ("Branch exp/<ID> — no PR yet")
//   4. nothing pushed        → "Being coded on <deviceLabel>" empty state
//                              opening the live steer viewer.
// The steer viewer (watch/steer/start-on-desktop) is always mounted so its
// Watch/Steer controls are present whenever a session is running, on every tier.

interface IssueChangesTabProps {
  issue: Issue
  workspaceId: string
  currentUserId: string
  users: User[]
}

type BranchState =
  | { kind: `loading` }
  | { kind: `files`; files: PullFile[] }
  | { kind: `none` } // branch was never pushed (GitHub 404)
  | { kind: `error`; message: string }

export function IssueChangesTab({
  issue,
  workspaceId,
  currentUserId,
  users,
}: IssueChangesTabProps) {
  // Key the PR tier on prUrl (non-empty) so all four clients share the
  // predicate (iOS/Android/desktop key on prUrl too).
  const hasPr = Boolean(issue.prUrl)
  const branchName = `exp/${issue.identifier}`

  // The running coding session (if any) drives the tier-4 device label. The
  // steer viewer below runs its own identical query for its controls; this one
  // only needs the label + "is anything running" for the empty state.
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) =>
          and(eq(s.issueId, issue.id), eq(s.status, `running`))
        ),
    [issue.id]
  )
  // Staleness guard (EXP-153): heartbeat-dead `running` rows render as absent.
  const now = useNow()
  const sessions = ((sessionRows ?? []) as CodingSession[]).filter(
    (s) => !isCodingSessionStale(s.updatedAt, now)
  )
  const runningSession = useMemo(() => {
    if (sessions.length === 0) return null
    return sessions.reduce((latest, row) =>
      new Date(row.startedAt) > new Date(latest.startedAt) ? row : latest
    )
  }, [sessions])

  const [branch, setBranch] = useState<BranchState>({ kind: `loading` })

  // Tier 3 lookup: only when there is no PR (the PR diff supersedes it). The
  // proc returns null when the branch was never pushed. Refetch on demand and
  // whenever the issue changes (mount == first fetch; §4.8 freshness).
  const loadBranchDiff = useCallback(() => {
    if (hasPr) return () => {}
    let cancelled = false
    setBranch({ kind: `loading` })
    trpc.repositories.branchDiff
      .query({ issueId: issue.id })
      .then((res) => {
        if (cancelled) return
        if (!res) {
          setBranch({ kind: `none` })
          return
        }
        setBranch({ kind: `files`, files: res.files })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setBranch({
          kind: `error`,
          message: err instanceof Error ? err.message : `Failed to load branch`,
        })
      })
    return () => {
      cancelled = true
    }
  }, [hasPr, issue.id])

  useEffect(() => loadBranchDiff(), [loadBranchDiff])

  const steerViewer =
    currentUserId ? (
      <IssueSteerPanel
        issueId={issue.id}
        workspaceId={workspaceId}
        currentUserId={currentUserId}
        users={users}
      />
    ) : null

  // ── Tier 2: PR diff ─────────────────────────────────────────────────────
  if (hasPr) {
    return (
      <div className="flex flex-col">
        <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-sm min-w-0">
          <GitPullRequest className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate">
            Pull request{` `}
            {issue.prUrl ? (
              <a
                href={issue.prUrl}
                target="_blank"
                rel="noreferrer"
                className="font-mono hover:underline"
              >
                #{issue.prNumber}
              </a>
            ) : (
              <span className="font-mono">#{issue.prNumber}</span>
            )}
          </span>
        </div>
        <DiffView issueId={issue.id} />
        {steerViewer}
      </div>
    )
  }

  // ── Tier 3: branch pushed, no PR ────────────────────────────────────────
  if (branch.kind === `files` && branch.files.length > 0) {
    return (
      <div className="flex flex-col">
        <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-sm min-w-0">
          <GitBranch className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate">
            Branch <span className="font-mono">{branchName}</span>
            {` — no PR yet`}
          </span>
          <Button
            variant="ghost"
            size="icon-xs"
            className="ml-auto shrink-0 text-muted-foreground"
            aria-label="Refresh changes"
            onClick={() => loadBranchDiff()}
          >
            <RotateCw className="size-3.5" />
          </Button>
        </div>
        <FileDiffList files={branch.files} />
        {steerViewer}
      </div>
    )
  }

  if (branch.kind === `loading`) {
    return (
      <div className="flex flex-col">
        <div className="flex items-center gap-2 px-4 py-3 text-xs text-muted-foreground">
          <Loader2 className="size-3.5 animate-spin" /> Loading changes…
        </div>
        {steerViewer}
      </div>
    )
  }

  // ── Tier 4 / empty ──────────────────────────────────────────────────────
  // A session is running but nothing has been pushed yet: name the machine and
  // let the steer viewer below provide Watch/Steer. Otherwise a plain empty
  // state (or a branch-lookup error, non-fatal).
  return (
    <div className="flex flex-col">
      {runningSession ? (
        <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-sm min-w-0">
          <GitBranch className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate text-muted-foreground">
            Being coded on{` `}
            {runningSession.deviceLabel ?? `a desktop`} — Watch / Steer below
          </span>
        </div>
      ) : (
        <div className="px-4 py-6 text-xs text-muted-foreground">
          {branch.kind === `error`
            ? `Couldn’t load branch changes: ${branch.message}`
            : `No changes yet. A pushed branch or pull request will appear here.`}
        </div>
      )}
      {steerViewer}
    </div>
  )
}
