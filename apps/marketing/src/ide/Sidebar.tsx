/* ─── Left sidebar panel — switches by active rail tool ─── */
import {
  INBOX_ITEMS,
  ISSUES,
  MY_ISSUE_IDS,
  PROJECT,
  REVIEWS,
  type InboxType,
} from "./data"
import { useIde } from "./state"
import { BoardPanel, IssueRow } from "./Board"
import { FilesPanel } from "./Files"
import { ScPanel } from "./SourceControl"
import { ToolHead } from "./bits"
import {
  IcCircleDot,
  IcCircleUser,
  IcGitMerge,
  IcGitPullRequest,
  IcInbox,
  IcMessageSquare,
  IcUserPlus,
  type IdeIcon,
} from "./icons"

/* Notification-type → icon (Linear-style single activity stream) */
const inboxIcon: Record<InboxType, IdeIcon> = {
  issue_assigned: IcUserPlus,
  issue_comment: IcMessageSquare,
  issue_status_changed: IcCircleDot,
  pr_opened: IcGitPullRequest,
  pr_merged: IcGitMerge,
}

function InboxPanel() {
  const { interactive, inboxRead, markInboxRead, markAllInboxRead, openIssue } = useIde()
  const unreadLeft = INBOX_ITEMS.some((n) => n.unread && !inboxRead.has(n.id))
  return (
    <div className="ide-inboxpanel">
      <ToolHead
        icon={<IcInbox size={13} className="ide-c-muted" />}
        title="Inbox"
        trailing={
          unreadLeft ? (
            <button
              className={`ide-ghost${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={interactive ? markAllInboxRead : undefined}
            >
              Mark all read
            </button>
          ) : undefined
        }
      />
      <div className="ide-inbox-list">
        {INBOX_ITEMS.map((n) => {
          const Icon = inboxIcon[n.type]
          const unread = n.unread && !inboxRead.has(n.id)
          return (
            <div
              key={n.id}
              className={`ide-inbox-row${unread ? `` : ` is-read`}${interactive ? ` is-click` : ``}`}
              onClick={
                interactive
                  ? () => {
                      markInboxRead(n.id)
                      openIssue(n.issueId)
                    }
                  : undefined
              }
            >
              <span className="ide-inbox-badge">
                <Icon size={13} />
              </span>
              <div className="ide-inbox-main">
                <div className="ide-inbox-line1">
                  <span className="ide-inbox-id">{n.issueId}</span>
                  <span className={`ide-inbox-title${unread ? ` is-unread` : ``}`}>
                    {n.title}
                  </span>
                </div>
                <div className="ide-inbox-sentence">{n.sentence}</div>
              </div>
              <div className="ide-inbox-meta">
                <span className="ide-inbox-time">{n.time}</span>
                {unread && <span className="ide-inbox-dot" />}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

function MyIssuesPanel() {
  const mine = ISSUES.filter((i) => MY_ISSUE_IDS.includes(i.id))
  return (
    <div className="ide-inboxpanel">
      <ToolHead
        icon={<IcCircleUser size={13} className="ide-c-muted" />}
        title="My Issues"
      />
      <div className="ide-board-list">
        {mine.map((i) => (
          <IssueRow key={i.id} issue={i} />
        ))}
      </div>
    </div>
  )
}

function ReviewsPanel() {
  const { interactive, mergedReviews, goneReviews, mergeReview, openIssue } = useIde()
  const visible = REVIEWS.filter((r) => !goneReviews.has(r.issueId))
  return (
    <div className="ide-inboxpanel">
      <ToolHead
        icon={<IcGitPullRequest size={13} className="ide-c-muted" />}
        title="Reviews"
      />
      {visible.length === 0 ? (
        <div className="ide-reviews-empty">No open pull requests.</div>
      ) : (
        <div className="ide-reviews-group">
          <div className="ide-reviews-project">
            <span className="ide-proj-dot" style={{ background: PROJECT.color }} />
            {PROJECT.name}
          </div>
          {visible.map((r) => {
            const merged = mergedReviews.has(r.issueId)
            return (
              <div
                key={r.issueId}
                className={`ide-review-row${merged ? ` is-merged` : ``}${interactive ? ` is-click` : ``}`}
                onClick={interactive ? () => openIssue(r.issueId) : undefined}
              >
                {merged ? (
                  <IcGitMerge size={14} className="ide-review-icon ide-c-blue" />
                ) : (
                  <IcGitPullRequest size={14} className="ide-review-icon ide-c-green" />
                )}
                <div className="ide-review-main">
                  <div className="ide-review-line1">
                    <span className="ide-review-id">{r.identifier}</span>
                    <span className="ide-review-title">{r.title}</span>
                  </div>
                  <div className="ide-review-sub">{`#${r.prNumber} · ${r.branch}`}</div>
                </div>
                {merged ? (
                  <span className="ide-review-mergedtag">Merged ✓</span>
                ) : (
                  <button
                    className={`ide-btn-sm ide-btn-plain ide-review-mergebtn${interactive ? ` is-click` : ``}`}
                    type="button"
                    onClick={
                      interactive
                        ? (e) => {
                            e.stopPropagation()
                            mergeReview(r.issueId)
                          }
                        : undefined
                    }
                  >
                    Merge
                  </button>
                )}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

export function SidebarPanel() {
  const { tool } = useIde()
  const wide = tool === `issues` || tool === `my-issues` || tool === `reviews` || tool === `inbox`
  return (
    <div className={`ide-sidebar${wide ? ` ide-sidebar-wide` : ``}`}>
      {tool === `issues` ? (
        <BoardPanel />
      ) : tool === `files` ? (
        <FilesPanel />
      ) : tool === `source-control` ? (
        <ScPanel />
      ) : tool === `inbox` ? (
        <InboxPanel />
      ) : tool === `my-issues` ? (
        <MyIssuesPanel />
      ) : (
        <ReviewsPanel />
      )}
    </div>
  )
}
