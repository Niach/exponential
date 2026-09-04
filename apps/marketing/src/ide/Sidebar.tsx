/* ─── Left sidebar panel — switches by active rail tool ─── */
import { useState } from "react"
import {
  INBOX_ITEMS,
  ISSUES,
  MY_ISSUE_IDS,
  REVIEWS,
  type InboxType,
} from "./data"
import { useIde } from "./state"
import { BoardPanel, IssueRow } from "./Board"
import { FilesPanel } from "./Files"
import { ScPanel } from "./SourceControl"
import { ACTIVE_BOARD } from "./Rail"
import {
  IcCircleDot,
  IcCircleUser,
  IcGitMerge,
  IcGitPullRequest,
  IcInbox,
  IcListChecks,
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

/* The Inbox tool window (EXP-186/282/698): ONE personal surface with an
   Inbox tab (the notification stream) and a My Issues tab (the board pinned
   to assignee == me), switched by a left-aligned strip of small SELECT pills.
   "Mark all read" rides the strip's trailing edge as an ICON-ONLY ghost
   button (NOTIFICATION_MARK_READ = list-checks) and only while something is
   unread. */
function InboxPanel() {
  const { interactive, inboxRead, markInboxRead, markAllInboxRead, openIssue } = useIde()
  const [tab, setTab] = useState<`inbox` | `my-issues`>(`inbox`)
  const unreadLeft = INBOX_ITEMS.some((n) => n.unread && !inboxRead.has(n.id))
  const mine = ISSUES.filter((i) => MY_ISSUE_IDS.includes(i.id))
  return (
    <div className="ide-inboxpanel">
      <div className="ide-tooltabs">
        <button
          className={`ide-tooltab${tab === `inbox` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? () => setTab(`inbox`) : undefined}
        >
          <IcInbox size={10} />
          Inbox
        </button>
        <button
          className={`ide-tooltab${tab === `my-issues` ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? () => setTab(`my-issues`) : undefined}
        >
          <IcCircleUser size={10} />
          My Issues
        </button>
        {tab === `inbox` && unreadLeft && (
          <span className="ide-tooltabs-trailing">
            <button
              className={`ide-icbtn${interactive ? ` is-click` : ``}`}
              type="button"
              title="Mark all read"
              onClick={interactive ? markAllInboxRead : undefined}
            >
              <IcListChecks size={11} />
            </button>
          </span>
        )}
      </div>
      {tab === `my-issues` ? (
        <div className="ide-board-list">
          {mine.map((i) => (
            <IssueRow key={i.id} issue={i} />
          ))}
        </div>
      ) : (
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
                  <Icon size={10} />
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
      )}
    </div>
  )
}

/* ─── Reviews (EXP-706) — a full-page SCREEN, not a docked tool window:
   the cross-board open-PR queue, grouped by board, each row a glass card
   with the PR glyph, identifier, title, its branch and the confirmed
   one-click squash merge (reviews_view.rs `pull_row`/`review_row`). ─── */
export function ReviewsScreen() {
  const { interactive, mergedReviews, goneReviews, mergeReview, openIssue } = useIde()
  const [armed, setArmed] = useState<string | null>(null)
  const visible = REVIEWS.filter((r) => !goneReviews.has(r.issueId))
  if (visible.length === 0) {
    return (
      <div className="ide-reviews-screen">
        <div className="ide-reviews-empty">
          <span className="ide-reviews-empty-title">No open pull requests</span>
          <span className="ide-reviews-empty-sub">
            Open pull requests in this team&apos;s repositories land here for review.
          </span>
        </div>
      </div>
    )
  }
  return (
    <div className="ide-reviews-screen">
      <div className="ide-reviews-group">
        <div className="ide-reviews-project">
          <ACTIVE_BOARD.Icon size={11} style={{ color: ACTIVE_BOARD.color }} />
          <span className="ide-reviews-project-name">{ACTIVE_BOARD.name}</span>
          <span className="ide-reviews-project-count">{visible.length}</span>
        </div>
        {visible.map((r) => {
          const merged = mergedReviews.has(r.issueId)
          const isArmed = armed === r.issueId
          return (
            <div
              key={r.issueId}
              className={`ide-review-row${merged ? ` is-merged` : ``}${interactive ? ` is-click` : ``}`}
              onClick={interactive ? () => openIssue(r.issueId) : undefined}
            >
              <div className="ide-review-main">
                <div className="ide-review-line1">
                  {merged ? (
                    <IcGitMerge size={11} className="ide-review-icon ide-c-blue" />
                  ) : (
                    <IcGitPullRequest size={11} className="ide-review-icon ide-c-green" />
                  )}
                  <span className="ide-review-id">{r.identifier}</span>
                  <span className="ide-review-title">{r.title}</span>
                </div>
                {/* EXP-706: the sub-line is the BRANCH — the PR number rides
                    the identifier column on GitHub, never here. */}
                <div className="ide-review-sub">{r.branch}</div>
              </div>
              {merged ? (
                <span className="ide-review-mergedtag">Merged</span>
              ) : (
                <button
                  className={`ide-btn-outline ide-review-mergebtn${isArmed ? ` is-danger` : ``}${interactive ? ` is-click` : ``}`}
                  type="button"
                  onClick={
                    interactive
                      ? (e) => {
                          e.stopPropagation()
                          /* pr_merge::two_click — Merge arms, Confirm merge fires. */
                          if (isArmed) {
                            setArmed(null)
                            mergeReview(r.issueId)
                          } else {
                            setArmed(r.issueId)
                          }
                        }
                      : undefined
                  }
                >
                  {isArmed ? (
                    `Confirm merge`
                  ) : (
                    <>
                      <IcGitMerge size={11} />
                      Merge
                    </>
                  )}
                </button>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}

/* Reviews is a rail SCREEN (EXP-706) — it replaces the whole content area,
   tool window included, so it never reaches this switch. */
export function SidebarPanel() {
  const { tool } = useIde()
  return (
    <div className="ide-sidebar">
      {tool === `files` ? (
        <FilesPanel />
      ) : tool === `source-control` ? (
        <ScPanel />
      ) : tool === `inbox` ? (
        <InboxPanel />
      ) : (
        <BoardPanel />
      )}
    </div>
  )
}
