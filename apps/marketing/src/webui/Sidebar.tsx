/* ─── Web sidebar ───
   Mirrors apps/web components/team/sidebar.tsx on the shadcn sidebar
   primitive (packages/ui/src/sidebar.tsx): a 17rem (314.5px) transparent
   column. Header = team switcher + icon-only Search and New-issue actions
   (EXP-449), one Separator under it. Nav (EXP-1293, ×4 with the IDE rail):
   Agent, Inbox, Threads, Devices (Drafts only while a draft exists, so the
   demo has none). Then the Agents group (EXP-1293), the Apps group (VAPP-110:
   the built-in Devices app first), Boards, and the Active group (the team's
   live threads, EXP-923/1318). Footer: Getting started + the user row and
   the settings gear. Menu rows are `density="compact"` (h-7). */
import { getIssue, INBOX_ITEMS } from "../ide/data"
import { useWeb, type WebNav } from "./state"
import {
  AGENT_SESSIONS,
  WEB_AGENTS,
  WEB_APPS,
  WEB_PROJECTS,
  WEB_TEAM,
  WEB_USER,
  type DemoAgentDef,
  type DemoApp,
  type DemoProjectIcon,
} from "./data"
import { AgentRunMarkDemo, WebUserAvatar } from "./bits"
import {
  ICON_35,
  ICON_4,
  IcAgent,
  IcBug,
  IcChartLine,
  IcCode,
  IcCompose,
  IcDevices,
  IcInbox,
  IcKanban,
  IcMegaphone,
  IcPackage,
  IcPlus,
  IcRocket,
  IcSearch,
  IcSettings,
  IcSparkles,
  IcTeamSwitcher,
  IcThreads,
  type WebIcon,
} from "./icons"

const boardIcon: Record<DemoProjectIcon, WebIcon> = {
  code: IcCode,
  kanban: IcKanban,
  megaphone: IcMegaphone,
}

const agentIcon: Record<DemoAgentDef[`icon`], WebIcon> = {
  bug: IcBug,
  package: IcPackage,
}

const appIcon: Record<DemoApp[`icon`], WebIcon> = {
  devices: IcDevices,
  rocket: IcRocket,
  "chart-line": IcChartLine,
}

/* SidebarMenuButton density="compact": h-7, rounded-full, p-2, gap-2,
   text-sm. A nav badge (NavDot, EXP-699) is a sibling absolutely placed at
   right-2: `bg-primary` unread, green-500 live, yellow-400 needs you; an
   agent's needs-you dot is amber-500. */
type Dot = `primary` | `green` | `yellow` | `amber`

function NavItem({
  icon,
  label,
  active,
  onClick,
  dot,
  muted,
  trailing,
  tall,
}: {
  icon: React.ReactNode
  label: string
  active?: boolean
  onClick?: () => void
  dot?: Dot
  muted?: boolean
  trailing?: React.ReactNode
  /* The footer's Getting started row keeps the default h-8. */
  tall?: boolean
}) {
  const { interactive } = useWeb()
  const clickable = interactive && onClick
  return (
    <div className="web-nav-item-wrap">
      <button
        className={`web-nav-item${tall ? ` is-tall` : ``}${active ? ` is-active` : ``}${muted ? ` is-muted` : ``}${clickable ? ` is-click` : ``}`}
        type="button"
        onClick={clickable ? onClick : undefined}
      >
        {icon}
        <span className="web-nav-label">{label}</span>
        {trailing}
      </button>
      {dot && <span className={`web-nav-dot is-${dot}`} />}
    </div>
  )
}

function GroupLabel({ label, action }: { label: string; action?: string }) {
  return (
    <>
      <div className="web-group-label">{label}</div>
      {action && (
        <button className="web-group-action is-click" type="button" title={action}>
          <IcPlus size={ICON_4} />
        </button>
      )}
    </>
  )
}

export function WebSidebar() {
  const { nav, setNav, closeIssue, inboxRead } = useWeb()

  const unread = INBOX_ITEMS.filter((n) => n.unread && !inboxRead.has(n.id)).length
  /* threadsNavState: amber while a thread needs you, else green while one
     runs. */
  const threadsLive = AGENT_SESSIONS.length > 0

  const go = (target: WebNav) => () => {
    setNav(target)
    closeIssue()
  }

  return (
    <div className="web-sidebar">
      <div className="web-side-head">
        <button className="web-team is-click" type="button">
          <span className="web-team-avatar">{WEB_TEAM.name[0]}</span>
          <span className="web-team-name">{WEB_TEAM.name}</span>
          <IcTeamSwitcher size={ICON_4} className="web-team-chev" />
        </button>
        <button className="web-headbtn is-click" type="button" title="Search">
          <IcSearch size={ICON_4} />
        </button>
        <button
          className="web-headbtn is-primary is-click"
          type="button"
          title="New issue"
        >
          <IcCompose size={ICON_4} />
        </button>
      </div>
      <div className="web-side-rule" />

      <div className="web-side-scroll">
        <div className="web-side-group">
          <NavItem
            icon={<IcAgent size={ICON_4} />}
            label="Agent"
            active={nav === `agent`}
            onClick={go(`agent`)}
          />
          <NavItem
            icon={<IcInbox size={ICON_4} />}
            label="Inbox"
            active={nav === `inbox`}
            onClick={go(`inbox`)}
            dot={unread > 0 ? `primary` : undefined}
          />
          <NavItem
            icon={<IcThreads size={ICON_4} />}
            label="Threads"
            dot={threadsLive ? `green` : undefined}
          />
          <NavItem icon={<IcDevices size={ICON_4} />} label="Devices" />
        </div>

        <div className="web-side-group">
          <GroupLabel label="Agents" action="New agent" />
          {WEB_AGENTS.map((agent) => {
            const Icon = agentIcon[agent.icon]
            return (
              <NavItem
                key={agent.name}
                icon={<Icon size={ICON_4} style={{ color: agent.color }} />}
                label={agent.name}
                dot={agent.needsYou ? `amber` : undefined}
              />
            )
          })}
        </div>

        <div className="web-side-group">
          <GroupLabel label="Apps" action="New app" />
          {WEB_APPS.map((app) => {
            const Icon = appIcon[app.icon]
            return (
              <NavItem
                key={app.name}
                icon={<Icon size={ICON_4} style={app.color ? { color: app.color } : undefined} />}
                label={app.name}
              />
            )
          })}
        </div>

        <div className="web-side-group">
          <GroupLabel label="Boards" action="Create board" />
          {WEB_PROJECTS.map((board, i) => {
            const Icon = boardIcon[board.icon]
            /* The demo board is the first (dogfood) one. */
            const isBoard = i === 0
            return (
              <NavItem
                key={board.slug}
                icon={<Icon size={ICON_4} style={{ color: board.color }} />}
                label={board.name}
                active={nav === `board` && isBoard}
                onClick={isBoard ? go(`board`) : undefined}
              />
            )
          })}
        </div>

        {/* The Active group: the team's live threads, the run mark, the
            thread's name, the root's device glyph trailing. */}
        <div className="web-side-group">
          <GroupLabel label="Active" />
          {AGENT_SESSIONS.map((s) => {
            const issue = getIssue(s.issueId)
            return (
              <NavItem
                key={s.id}
                icon={<AgentRunMarkDemo agent={s.agent} state={s.state} />}
                label={`#${issue.id} ${issue.title}`}
                trailing={
                  <span className="web-nav-device">
                    <IcDevices size={ICON_35} />
                  </span>
                }
              />
            )
          })}
        </div>
      </div>

      <div className="web-side-footer">
        <NavItem icon={<IcSparkles size={ICON_4} />} label="Getting started" muted tall />
        <div className="web-userrow">
          <button className="web-user is-click" type="button">
            <WebUserAvatar name={WEB_USER.name} initials={WEB_USER.initials} size={27.75} fontSize={13.875} />
            <span className="web-user-name">{WEB_USER.firstName}</span>
            <IcTeamSwitcher size={ICON_4} className="web-team-chev" />
          </button>
          <button className="web-user-gear is-click" type="button" title="Settings">
            <IcSettings size={ICON_4} />
          </button>
        </div>
      </div>
    </div>
  )
}
