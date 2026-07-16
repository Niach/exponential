/* ─── Inbox — Linear-style notification stream in the web chrome ───
   Mirrors apps/web inbox/inbox-view.tsx: centered column, type icon in a
   muted circle, identifier + title + time + unread dot, sentence line,
   read rows fade, "Mark all read" in the header. */
import { INBOX_ITEMS, type InboxType } from "../ide/data"
import { useWeb } from "./state"
import {
  IcBell,
  IcCircleDot,
  IcGitMerge,
  IcGitPullRequest,
  IcMessageSquare,
  IcUserPlus,
  type IdeIcon,
} from "../ide/icons"

const typeIcon: Record<InboxType, IdeIcon> = {
  issue_assigned: IcUserPlus,
  issue_comment: IcMessageSquare,
  issue_status_changed: IcCircleDot,
  pr_opened: IcGitPullRequest,
  pr_merged: IcGitMerge,
}

export function WebInbox() {
  const { interactive, inboxRead, markInboxRead, markAllInboxRead, setNav, openIssue } = useWeb()
  const unreadLeft = INBOX_ITEMS.some((n) => n.unread && !inboxRead.has(n.id))
  return (
    <div className="web-inbox">
      <div className="web-inbox-col">
        <div className="web-inbox-head">
          <span className="web-inbox-title">
            <IcBell size={15} />
            Inbox
          </span>
          {unreadLeft && (
            <button
              className={`web-ghost${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={interactive ? markAllInboxRead : undefined}
            >
              Mark all read
            </button>
          )}
        </div>
        <div className="web-inbox-list">
          {INBOX_ITEMS.map((n) => {
            const Icon = typeIcon[n.type]
            const unread = n.unread && !inboxRead.has(n.id)
            return (
              <button
                key={n.id}
                type="button"
                className={`web-inbox-card${unread ? `` : ` is-read`}${interactive ? ` is-click` : ``}`}
                onClick={
                  interactive
                    ? () => {
                        markInboxRead(n.id)
                        setNav(`project`)
                        openIssue(n.issueId)
                      }
                    : undefined
                }
              >
                <span className="web-inbox-badge">
                  <Icon size={13} />
                </span>
                <span className="web-inbox-main">
                  <span className="web-inbox-line1">
                    <span className="web-inbox-id">{n.issueId}</span>
                    <span className={`web-inbox-issue${unread ? ` is-unread` : ``}`}>
                      {n.title}
                    </span>
                    <span className="web-inbox-time">{n.time}</span>
                    {unread && <span className="web-inbox-dot" />}
                  </span>
                  <span className="web-inbox-sentence">{n.sentence}</span>
                </span>
              </button>
            )
          })}
        </div>
      </div>
    </div>
  )
}
