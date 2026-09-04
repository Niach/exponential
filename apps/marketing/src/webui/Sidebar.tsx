/* ─── Web sidebar ───
   Mirrors apps/web components/team/sidebar.tsx on the shadcn sidebar
   primitive: a 16rem (296px) transparent rail divided from the main pane by
   one hairline. Header = team switcher + icon-only Search and New-issue
   actions (EXP-449). Nav = Inbox / Support / Devices / Actions / Automations /
   Reviews, badged with DOTS, never counts (EXP-699). Then the Boards group,
   and a footer with Getting started and the user row + settings gear. */
import { INBOX_ITEMS, REVIEWS } from "../ide/data"
import { useWeb, type WebNav } from "./state"
import {
  AGENTS_RUNNING,
  SUPPORT_THREADS,
  WEB_PROJECTS,
  WEB_USER,
  type DemoProjectIcon,
} from "./data"
import {
  ICON_4,
  IcActions,
  IcAutomations,
  IcCode,
  IcCompose,
  IcDevices,
  IcInbox,
  IcKanban,
  IcMegaphone,
  IcPlus,
  IcReviews,
  IcSearch,
  IcSettings,
  IcSparkles,
  IcSupport,
  IcTeamSwitcher,
  type WebIcon,
} from "./icons"

const boardIcon: Record<DemoProjectIcon, WebIcon> = {
  code: IcCode,
  kanban: IcKanban,
  megaphone: IcMegaphone,
}

/* SidebarMenuButton: h-8, rounded-full, p-2, gap-2, text-sm. EXP-699: the
   badge is a DOT, never a count — a sibling absolutely positioned at right-2,
   so the label truncates against the rail edge. */
function NavItem({
  icon,
  label,
  active,
  onClick,
  dot,
  muted,
}: {
  icon: React.ReactNode
  label: string
  active?: boolean
  onClick?: () => void
  /* `primary` (unread) or `green` (something live) — the app's two dot tints. */
  dot?: `primary` | `green`
  muted?: boolean
}) {
  const { interactive } = useWeb()
  const clickable = interactive && onClick
  return (
    <div className="web-nav-item-wrap">
      <button
        className={`web-nav-item${active ? ` is-active` : ``}${muted ? ` is-muted` : ``}${clickable ? ` is-click` : ``}`}
        type="button"
        onClick={clickable ? onClick : undefined}
      >
        {icon}
        <span className="web-nav-label">{label}</span>
      </button>
      {dot && <span className={`web-nav-dot is-${dot}`} />}
    </div>
  )
}

export function WebSidebar() {
  const { nav, setNav, closeIssue, inboxRead, threadRead } = useWeb()

  const unread = INBOX_ITEMS.filter((n) => n.unread && !inboxRead.has(n.id)).length
  /* DISTINCT open PRs, like the real ReviewsOpenBadge (a batch PR linked to
     several issues counts once) — only its presence lights the dot. */
  const hasOpenPrs = new Set(REVIEWS.map((r) => r.prNumber)).size > 0
  /* Unread helpdesk activity in THIS team, like SupportUnreadBadge. */
  const supportUnread = SUPPORT_THREADS.some(
    (t) => t.unread && !threadRead.has(t.id)
  )

  const go = (target: WebNav) => () => {
    setNav(target)
    closeIssue()
  }

  return (
    <div className="web-sidebar">
      <div className="web-side-head">
        <button className="web-team is-click" type="button">
          <span className="web-team-avatar">{WEB_PROJECTS[0].name[0]}</span>
          <span className="web-team-name">{WEB_PROJECTS[0].name}</span>
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
        {/* EXP-699/EXP-686 nav order: Inbox, Support, then the three surfaces
            the old Agents entry bundled (Devices, Actions, Automations), then
            Reviews. Badges are dots: primary for unread, green for live. */}
        <div className="web-side-group">
          <NavItem
            icon={<IcInbox size={ICON_4} />}
            label="Inbox"
            active={nav === `inbox`}
            onClick={go(`inbox`)}
            dot={unread > 0 ? `primary` : undefined}
          />
          <NavItem
            icon={<IcSupport size={ICON_4} />}
            label="Support"
            active={nav === `support`}
            onClick={go(`support`)}
            dot={supportUnread ? `primary` : undefined}
          />
          <NavItem
            icon={<IcDevices size={ICON_4} />}
            label="Devices"
            dot={AGENTS_RUNNING > 0 ? `green` : undefined}
          />
          <NavItem icon={<IcActions size={ICON_4} />} label="Actions" />
          <NavItem
            icon={<IcAutomations size={ICON_4} />}
            label="Automations"
          />
          <NavItem
            icon={<IcReviews size={ICON_4} />}
            label="Reviews"
            dot={hasOpenPrs ? `green` : undefined}
          />
        </div>

        <div className="web-side-group">
          <div className="web-group-label">Boards</div>
          <button className="web-group-action is-click" type="button" title="Create board">
            <IcPlus size={ICON_4} />
          </button>
          {WEB_PROJECTS.map((board, i) => {
            const Icon = boardIcon[board.icon]
            /* The demo board is the first (dogfood) one. */
            const isBoard = i === 0
            return (
              <NavItem
                key={board.slug}
                icon={<Icon size={ICON_4} style={{ color: board.color }} />}
                label={board.name}
                active={nav === `project` && isBoard}
                onClick={isBoard ? go(`project`) : undefined}
              />
            )
          })}
        </div>
      </div>

      <div className="web-side-footer">
        <NavItem icon={<IcSparkles size={ICON_4} />} label="Getting started" muted />
        <div className="web-userrow">
          <button className="web-user is-click" type="button">
            <span className="web-user-avatar">{WEB_USER.initials}</span>
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
