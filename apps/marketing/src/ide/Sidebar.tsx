/* ─── The Inbox and the Chats page.
   Inbox (sidebar.rs ListPanel, ListMode::Nav, EXP-186/1192): ALWAYS three
   columns — the rail, then the notifications as a 352px second sidebar
   INSIDE the content card (the Inbox | My issues segmented strip and the
   Mark all read glyph on top; a row = the notification glyph · mono
   identifier · title, semibold while unread, the primary dot), then the
   main pane: the "No notification selected" empty state until a row is
   picked, then its issue beside the list.
   Chats (threads_view.rs + chat_list.rs, chats.json LIST/WIDE, EXP-1380):
   the agents' bubbles, the filter chips (All · Needs you · Running ·
   Unread), then the flat chat rows — the 52 lead disc, the title and its
   time, the preview and the row word. The old Needs you · Running · Recent
   bands left the page (the rail keeps its Active rows). ─── */
import { useState } from "react"
import { INBOX_ITEMS, RAIL_AGENTS, getIssue, type InboxType, type RailAgent } from "./data"
import { useIde } from "./state"
import { IssueDetail } from "./IssueDetail"
import { AGENT_GLYPH } from "./Rail"
import {
  IcAssignee,
  IcCircleCheck,
  IcClaude,
  IcComment,
  IcInbox,
  IcMarkRead,
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

/* chats.json FILTERS: select Pills, a count after a non-zero one
   (`chatFilterLabel`). */
type ChatFilter = `all` | `needs_you` | `running` | `unread`
const FILTER_LABEL: Record<ChatFilter, string> = {
  all: `All`,
  needs_you: `Needs you`,
  running: `Running`,
  unread: `Unread`,
}
const FILTERS: ChatFilter[] = [`all`, `needs_you`, `running`, `unread`]

/* The IDE's measures at the demo's 0.875 type scale (chats.json `shell`:
   the 52 lead disc, the 66 bubble with its 3px ring; the 768 column). */
const DISC = 45.5
const BUBBLE = 57.75
const RING = 2.625
const GLYPH = 21

/* chat_list.rs `agent_bubble`: the agent's glyph in its colour on bg muted
   inside a background gap, the glass hairline ring while nothing waits,
   the name under it. */
function AgentBubble({ agent }: { agent: RailAgent }) {
  const Glyph = AGENT_GLYPH[agent.icon]
  return (
    <div style={{ width: BUBBLE + 6, flex: `none`, display: `flex`, flexDirection: `column`, alignItems: `center`, gap: 5.25 }}>
      <span
        style={{
          width: BUBBLE,
          height: BUBBLE,
          borderRadius: 999,
          padding: RING,
          boxShadow: `0 0 0 1px var(--g-stroke-card)`,
          display: `flex`,
        }}
      >
        <span
          style={{
            flex: 1,
            borderRadius: 999,
            background: `rgba(255, 255, 255, 0.08)`,
            display: `flex`,
            alignItems: `center`,
            justifyContent: `center`,
          }}
        >
          <Glyph size={GLYPH} style={{ color: agent.color }} />
        </span>
      </span>
      <span className="ide-c-muted ide-truncate" style={{ width: `100%`, textAlign: `center`, fontSize: `var(--f-xs)` }}>
        {agent.name}
      </span>
    </div>
  )
}

/* chat_list.rs `chat_lead_disc`: needs you = the brand mark with the amber
   badge, working = the working mark, else the bare brand mark. */
function LeadDisc({ needsYou }: { needsYou: boolean }) {
  return (
    <span
      style={{
        position: `relative`,
        width: DISC,
        height: DISC,
        flex: `none`,
        borderRadius: 999,
        background: `rgba(255, 255, 255, 0.08)`,
        display: `flex`,
        alignItems: `center`,
        justifyContent: `center`,
      }}
    >
      <IcClaude size={GLYPH} />
      {needsYou && (
        <span
          style={{
            position: `absolute`,
            top: 7,
            right: 7,
            width: 12.25,
            height: 12.25,
            borderRadius: 999,
            border: `2px solid #2a2a2c`,
            background: `var(--ide-amber)`,
          }}
        />
      )}
    </span>
  )
}

/* The Chats page (threads_view.rs, chats.json WIDE, EXP-1373/1380): ONE
   centred column — the agents' bubbles, the filter chips, then the flat
   chat rows (no bands); the empty state under the chips. */
export function ChatsScreen() {
  const { thread, openThread, interactive } = useIde()
  const [filter, setFilter] = useState<ChatFilter>(`all`)
  const needsYou = thread?.state === `needs_you`
  const running = thread?.state === `working`
  const counts: Record<ChatFilter, number> = {
    all: 0,
    needs_you: needsYou ? 1 : 0,
    running: running ? 1 : 0,
    unread: 0,
  }
  const shown = thread !== null && (filter === `all` || counts[filter] > 0)

  /* chats.json `chatPreview`: the waiting question, else `working…` while
     busy, else the newest message (`You: ` for the person's). */
  let prefix: string | null = null
  let preview = `No messages yet`
  if (thread) {
    const last = [...thread.rows].reverse().find((r) => r.kind !== `trace`)
    if (needsYou && thread.question) {
      prefix = `Question: `
      preview = thread.question.text
    } else if (running) {
      preview = `working…`
    } else if (last?.kind === `user`) {
      prefix = `You: `
      preview = last.text
    } else if (last?.kind === `narration`) {
      preview = last.text
    }
  }
  const issue = thread ? getIssue(thread.issueId) : null

  return (
    <div style={{ height: `100%`, overflowY: `auto` }}>
      <div
        style={{
          maxWidth: 672,
          margin: `0 auto`,
          padding: 14,
          display: `flex`,
          flexDirection: `column`,
          gap: 14,
        }}
      >
        <div style={{ display: `flex`, gap: 10.5, paddingBottom: 3.5, overflowX: `auto` }}>
          {RAIL_AGENTS.map((agent) => (
            <AgentBubble key={agent.name} agent={agent} />
          ))}
        </div>
        <div style={{ display: `flex`, gap: 7 }}>
          {FILTERS.map((f) => (
            <button
              key={f}
              type="button"
              className={`ide-pill is-md${interactive ? ` is-click` : ``}`}
              style={{
                padding: `0 14px`,
                ...(f === filter
                  ? { background: `var(--g-fill-active)`, borderColor: `var(--g-stroke-active)`, color: `var(--ide-fg)` }
                  : null),
              }}
              onClick={interactive ? () => setFilter(f) : undefined}
            >
              {counts[f] > 0 ? `${FILTER_LABEL[f]} ${counts[f]}` : FILTER_LABEL[f]}
            </button>
          ))}
        </div>
        {shown && thread && issue ? (
          <button
            type="button"
            className={interactive ? `is-click` : undefined}
            style={{ display: `flex`, alignItems: `center`, gap: 14, width: `100%`, textAlign: `left` }}
            onClick={interactive ? openThread : undefined}
          >
            <LeadDisc needsYou={needsYou} />
            <span
              style={{
                flex: 1,
                minWidth: 0,
                padding: `12.25px 0`,
                display: `flex`,
                flexDirection: `column`,
                gap: 3.5,
                borderBottom: `1px solid var(--g-stroke-row)`,
              }}
            >
              <span style={{ display: `flex`, alignItems: `baseline`, gap: 7 }}>
                <span className="ide-truncate" style={{ flex: 1, minWidth: 0, fontSize: 13.125, fontWeight: 500 }}>
                  {`#${issue.id} ${issue.title}`}
                </span>
                <span className={needsYou ? `ide-c-amber` : `ide-c-muted`} style={{ flex: `none`, fontSize: `var(--f-xs)` }}>
                  16:37
                </span>
              </span>
              <span style={{ display: `flex`, alignItems: `center`, gap: 7 }}>
                <span
                  className="ide-c-muted"
                  style={{
                    flex: 1,
                    minWidth: 0,
                    fontSize: `var(--f-sm)`,
                    fontStyle: running ? `italic` : undefined,
                    display: `-webkit-box`,
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: `vertical`,
                    overflow: `hidden`,
                  }}
                >
                  {prefix && <span style={{ color: `rgba(250, 250, 250, 0.85)` }}>{prefix}</span>}
                  {preview}
                </span>
                {needsYou && (
                  <span className="ide-c-muted" style={{ flex: `none`, fontSize: `var(--f-xs)` }}>
                    question
                  </span>
                )}
              </span>
            </span>
          </button>
        ) : (
          <PageEmpty Icon={IcThreads} title="No sessions yet" />
        )}
      </div>
    </div>
  )
}
