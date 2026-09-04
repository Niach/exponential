/* ─── Inbox — the merged personal surface (EXP-186) ───
   ONE sidebar entry with two tabs: the notification stream
   (inbox/inbox-view.tsx) and cross-board My Issues (my-issues-view.tsx),
   switched by the capsule segmented control in the page header, with
   "Mark all read" opposite it. */
import { INBOX_ITEMS, type InboxType } from "../ide/data"
import { useWeb } from "./state"
import { WebAgentDock, WebMyIssues } from "./Board"
import {
  ICON_35,
  ICON_4,
  IcAssigned,
  IcAssignee,
  IcComment,
  IcFilter,
  IcInbox,
  IcMerged,
  IcReviews,
  IcStatusChanged,
  ICON_3,
  type WebIcon,
} from "./icons"

const typeIcon: Record<InboxType, WebIcon> = {
  issue_assigned: IcAssigned,
  issue_comment: IcComment,
  issue_status_changed: IcStatusChanged,
  pr_opened: IcReviews,
  pr_merged: IcMerged,
}

function NotificationList() {
  const { interactive, inboxRead, markInboxRead, setNav, openIssue } = useWeb()
  return (
    <div className="web-inbox-scroll">
      <div className="web-inbox-col">
        {INBOX_ITEMS.map((n) => {
          const Icon = typeIcon[n.type]
          const unread = n.unread && !inboxRead.has(n.id)
          return (
            <button
              key={n.id}
              type="button"
              className={`web-notif${unread ? `` : ` is-read`}${interactive ? ` is-click` : ``}`}
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
              <span className="web-notif-badge">
                <Icon size={ICON_35} />
              </span>
              <span className="web-notif-main">
                <span className="web-notif-line1">
                  <span className="web-notif-id">{n.issueId}</span>
                  <span className={`web-notif-title${unread ? ` is-unread` : ``}`}>
                    {n.title}
                  </span>
                  <span className="web-notif-time">{n.time}</span>
                  {/* The dot keeps its 8px slot whether or not it is lit. */}
                  <span className="web-notif-dotslot">
                    {unread && <span className="web-notif-dot" />}
                  </span>
                </span>
                <span className="web-notif-sentence">{n.sentence}</span>
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )
}

export function WebInbox() {
  const {
    interactive,
    inboxRead,
    markAllInboxRead,
    inboxTab,
    setInboxTab,
  } = useWeb()
  const unread = INBOX_ITEMS.filter((n) => n.unread && !inboxRead.has(n.id)).length
  const isMine = inboxTab === `my-issues`

  return (
    <div className="web-page">
      <div className="web-tabrow">
        <div className="web-seg">
          <button
            type="button"
            className={`web-seg-btn${isMine ? `` : ` is-active`}${interactive ? ` is-click` : ``}`}
            onClick={interactive ? () => setInboxTab(`inbox`) : undefined}
          >
            <IcInbox size={ICON_4} />
            Inbox
            {unread > 0 && <span className="web-seg-count">{unread}</span>}
          </button>
          <button
            type="button"
            className={`web-seg-btn${isMine ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
            onClick={interactive ? () => setInboxTab(`my-issues`) : undefined}
          >
            <IcAssignee size={ICON_4} />
            My Issues
          </button>
        </div>
        {isMine ? (
          <button className="web-xsbtn is-click" type="button">
            <IcFilter size={ICON_3} />
            Filter
          </button>
        ) : (
          unread > 0 && (
            <button
              className={`web-smbtn${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={interactive ? markAllInboxRead : undefined}
            >
              Mark all read
            </button>
          )
        )}
      </div>
      {isMine ? <WebMyIssues /> : <NotificationList />}
      <WebAgentDock />
    </div>
  )
}
