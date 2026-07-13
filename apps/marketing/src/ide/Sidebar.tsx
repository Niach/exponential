/* ─── Left sidebar panel — switches by active rail tool ─── */
import {
  INBOX_ITEMS,
  ISSUES,
  MY_ISSUE_IDS,
  PROJECT,
  RELEASES,
  REVIEWS,
  releaseProgress,
  releaseSubline,
  type InboxType,
  type Release,
} from "./data"
import { useIde } from "./state"
import { BoardPanel, IssueRow } from "./Board"
import { FilesPanel } from "./Files"
import { ScPanel } from "./SourceControl"
import { ToolHead } from "./bits"
import {
  IcCalDays,
  IcChevLeft,
  IcCircleDot,
  IcCircleUser,
  IcCircleX,
  IcGitMerge,
  IcGitPullRequest,
  IcInbox,
  IcMessageSquare,
  IcPlay,
  IcPlus,
  IcRocket,
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

/* ─── Releases tool — rocket list rows + in-panel release detail ─── */

function ReleaseCodingPill() {
  return (
    <span className="ide-codingpill">
      <span className="ide-codingdot" />
      Coding…
    </span>
  )
}

function ReleaseRow({ release }: { release: Release }) {
  const { interactive, selectRelease, coding, codingTarget } = useIde()
  const shipped = Boolean(release.shippedAt)
  const codingHere =
    coding === `running` && codingTarget?.kind === `release` && codingTarget.id === release.id
  return (
    <div
      className={`ide-release-row${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => selectRelease(release.id) : undefined}
    >
      <div className="ide-release-line1">
        <IcRocket size={13} className={shipped ? `ide-c-green` : `ide-c-muted`} />
        <span className="ide-release-name">{release.name}</span>
        {codingHere && <ReleaseCodingPill />}
        {shipped && <span className="ide-shippedpill">Shipped</span>}
      </div>
      <div className="ide-release-sub">{releaseSubline(release)}</div>
    </div>
  )
}

function ReleaseDetail({ release }: { release: Release }) {
  const { interactive, selectRelease, requestCoding, coding, codingTarget, stopCoding } = useIde()
  const shipped = Boolean(release.shippedAt)
  const { done, total } = releaseProgress(release)
  const fraction = total > 0 ? done / total : 0
  const issues = ISSUES.filter((i) => release.issueIds.includes(i.id))
  const codingHere =
    coding === `running` && codingTarget?.kind === `release` && codingTarget.id === release.id
  return (
    <div className="ide-inboxpanel">
      <div className="ide-toolhead">
        <button
          className={`ide-ghost ide-icbtn${interactive ? ` is-click` : ``}`}
          type="button"
          title="All releases"
          onClick={interactive ? () => selectRelease(null) : undefined}
        >
          <IcChevLeft size={14} />
        </button>
        <span className="ide-release-detail-name">{release.name}</span>
        <div className="ide-flex1" />
        <button className="ide-ghost" type="button" title="Add workspace issues to this release">
          <IcPlus size={12} />
          Add issues
        </button>
        {codingHere ? (
          <button
            className={`ide-ghost${interactive ? ` is-click` : ``}`}
            type="button"
            onClick={interactive ? stopCoding : undefined}
          >
            <IcCircleX size={12} className="ide-c-danger" />
            Stop
          </button>
        ) : (
          <button
            className={`ide-ghost${interactive ? ` is-click` : ``}`}
            type="button"
            title="Launch a Claude orchestrator on this release's issues"
            onClick={
              interactive ? () => requestCoding({ kind: `release`, id: release.id }) : undefined
            }
          >
            <IcPlay size={12} className="ide-c-green" />
            Start coding
          </button>
        )}
      </div>
      <div className="ide-release-summary">
        <div className="ide-release-meta">
          {shipped ? (
            <>
              <span className="ide-shippedpill">Shipped</span>
              <span>{`Shipped ${release.shippedAt}`}</span>
            </>
          ) : release.target ? (
            <span className="ide-release-target">
              <IcCalDays size={12} />
              {`Target ${release.target}`}
            </span>
          ) : null}
          {codingHere && <ReleaseCodingPill />}
        </div>
        <div className="ide-progress">
          <div className="ide-progress-fill" style={{ width: `${fraction * 100}%` }} />
        </div>
        <div className="ide-release-progress-label">{`${done} of ${total} done`}</div>
      </div>
      <div className="ide-board-list">
        {issues.map((i) => (
          <IssueRow key={i.id} issue={i} />
        ))}
      </div>
    </div>
  )
}

function ReleasesPanel() {
  const { selectedRelease } = useIde()
  const selected = RELEASES.find((r) => r.id === selectedRelease)
  if (selected) return <ReleaseDetail release={selected} />
  return (
    <div className="ide-inboxpanel">
      <ToolHead
        icon={<IcRocket size={13} className="ide-c-muted" />}
        title="Releases"
        trailing={
          <button className="ide-ghost ide-icbtn" type="button" title="New release">
            <IcPlus size={12} />
          </button>
        }
      />
      <div className="ide-inbox-list">
        {RELEASES.map((r) => (
          <ReleaseRow key={r.id} release={r} />
        ))}
      </div>
    </div>
  )
}

export function SidebarPanel() {
  const { tool } = useIde()
  const wide =
    tool === `issues` ||
    tool === `my-issues` ||
    tool === `reviews` ||
    tool === `inbox` ||
    tool === `releases`
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
      ) : tool === `releases` ? (
        <ReleasesPanel />
      ) : (
        <ReviewsPanel />
      )}
    </div>
  )
}
