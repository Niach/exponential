/* ─── The Inbox and the Threads page.
   Inbox (sidebar.rs ListPanel, ListMode::Nav, EXP-186/1192): ALWAYS three
   columns — the rail, then the notifications as a 352px second sidebar
   INSIDE the content card (the Inbox | My issues segmented strip and the
   Mark all read glyph on top; a row = the notification glyph · mono
   identifier · title, semibold while unread, the primary dot), then the
   main pane: the "No notification selected" empty state until a row is
   picked, then its issue beside the list.
   Threads (threads_view.rs, threads-list.json): the team's threads in
   filled group bands — Needs you · Running · Recent — over flat rows: the
   run mark, the thread title, the root's device glyph. ─── */
import { INBOX_ITEMS, getIssue, type InboxType } from "./data"
import { useIde } from "./state"
import { RunMark } from "./bits"
import { IssueDetail } from "./IssueDetail"
import {
  IcAlert,
  IcAssignee,
  IcCheck,
  IcCircleCheck,
  IcComment,
  IcInbox,
  IcMarkRead,
  IcMonitor,
  IcPrOpened,
  IcSourceControl,
  IcThreads,
  IcUserPlus,
  type IdeIcon,
} from "./icons"

/* icons.json `notification-*`. */
const TYPE_ICON: Record<InboxType, IdeIcon> = {
  issue_assigned: IcUserPlus,
  issue_comment: IcComment,
  issue_status_changed: IcCircleCheck,
  pr_opened: IcPrOpened,
  pr_merged: IcSourceControl,
}

/* empty-state.json `page`: the 48px disc around a 24px glyph, the
   semibold title — centred in the pane. */
export function PageEmpty({ Icon, title }: { Icon: IdeIcon; title: string }) {
  return (
    <div className="ide-empty">
      <span className="ide-empty-disc">
        <Icon size={24} />
      </span>
      <span className="ide-empty-title">{title}</span>
    </div>
  )
}

export function InboxNav() {
  const { interactive, inboxRead, markInboxRead, markAllInboxRead, openIssue, active, tabs } = useIde()
  const openRef = tabs.find((t) => t.key === active)?.ref ?? null
  return (
    <div className="ide-inboxnav">
      <div className="ide-inbox-head">
        <div className="ide-seg">
          <span className="ide-seg-item is-active">
            <IcInbox size={12.25} />
            Inbox
          </span>
          <span className="ide-seg-item">
            <IcAssignee size={12.25} />
            My issues
          </span>
        </div>
        <button
          className={`ide-ghost-btn is-sm${interactive ? ` is-click` : ``}`}
          type="button"
          title="Mark all read"
          onClick={interactive ? markAllInboxRead : undefined}
        >
          <IcMarkRead size={12.25} />
        </button>
      </div>
      <div className="ide-inbox-rows">
        {INBOX_ITEMS.map((n) => {
          const Icon = TYPE_ICON[n.type]
          const unread = n.unread && !inboxRead.has(n.id)
          return (
            <button
              key={n.id}
              className={`ide-inbox-row${unread ? ` is-unread` : ``}${openRef === n.issueId ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={
                interactive
                  ? () => {
                      markInboxRead(n.id)
                      openIssue(n.issueId, `inbox`)
                    }
                  : undefined
              }
            >
              <Icon size={12.25} className="ide-c-muted" />
              <span className="ide-inbox-id">{n.issueId}</span>
              <span className="ide-inbox-title">{n.title}</span>
              {unread && <span className="ide-inbox-dot" />}
            </button>
          )
        })}
      </div>
    </div>
  )
}

/* The Inbox screen: the list, then the empty pane (or the picked issue —
   Ide.tsx renders that case with the same list beside it). */
export function InboxScreen() {
  return (
    <>
      <InboxNav />
      <div className="ide-center">
        <PageEmpty Icon={IcInbox} title="No notification selected" />
      </div>
    </>
  )
}

export function InboxDetail({ issueId }: { issueId: string }) {
  return (
    <>
      <InboxNav />
      <div className="ide-center">
        <IssueDetail issueId={issueId} />
      </div>
    </>
  )
}

export function ThreadsScreen() {
  const { thread, openThread, interactive } = useIde()
  if (!thread) {
    return <PageEmpty Icon={IcThreads} title="No sessions yet" />
  }
  const issue = getIssue(thread.issueId)
  /* threads-list.json partitionThreads: needs_you, else running (busy),
     else recent. */
  const group =
    thread.state === `needs_you`
      ? { label: `Needs you`, Icon: IcAlert, tone: `ide-c-amber` }
      : thread.state === `working`
        ? { label: `Running`, Icon: null, tone: `` }
        : { label: `Recent`, Icon: IcCheck, tone: `ide-c-muted` }
  return (
    <div className="ide-threads">
      <div className="ide-threads-band">
        {group.Icon ? <group.Icon size={12.25} className={group.tone} /> : <RunMark size={12.25} />}
        {group.label}
      </div>
      <button
        className={`ide-threads-row${interactive ? ` is-click` : ``}`}
        type="button"
        onClick={interactive ? openThread : undefined}
      >
        <RunMark needsYou={thread.state === `needs_you`} />
        <span className="ide-threads-title">{`#${issue.id} ${issue.title}`}</span>
        <IcMonitor size={10.5} className="ide-c-muted" />
      </button>
    </div>
  )
}
