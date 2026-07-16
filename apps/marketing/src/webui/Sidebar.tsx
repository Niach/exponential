/* ─── Web sidebar — team switcher, nav, projects, footer ───
   Mirrors apps/web/src/components/workspace/sidebar.tsx (shadcn sidebar on
   the dark zinc theme): 256px rail on --bg-elev, 32px menu buttons, badges
   right-aligned, Projects group with colored type icons + a globe on the
   public feedback board, footer with Getting started / Send feedback / user. */
import { INBOX_ITEMS, PROJECT, REVIEWS } from "../ide/data"
import { useWeb, type WebNav } from "./state"
import { AGENTS_RUNNING, WEB_PROJECTS, WEB_USER, type DemoProjectIcon } from "./data"
import {
  IcChevsUpDown,
  IcCircleUser,
  IcGitPullRequest,
  IcInbox,
  IcPlus,
  IcSearch,
  type IdeIcon,
} from "../ide/icons"
import {
  IcBot,
  IcCode2,
  IcGlobe,
  IcKanban,
  IcLifeBuoy,
  IcMegaphone,
  IcSparkles,
} from "./icons"

const projectIcon: Record<DemoProjectIcon, IdeIcon> = {
  code: IcCode2,
  kanban: IcKanban,
  megaphone: IcMegaphone,
}

function NavItem({
  icon,
  label,
  active,
  onClick,
  trailing,
}: {
  icon: React.ReactNode
  label: string
  active?: boolean
  onClick?: () => void
  trailing?: React.ReactNode
}) {
  const { interactive } = useWeb()
  const clickable = interactive && onClick
  return (
    <button
      className={`web-nav-item${active ? ` is-active` : ``}${clickable ? ` is-click` : ``}`}
      type="button"
      onClick={clickable ? onClick : undefined}
    >
      {icon}
      <span className="web-nav-label">{label}</span>
      {trailing}
    </button>
  )
}

export function WebSidebar() {
  const { nav, setNav, closeIssue, inboxRead } = useWeb()

  const unread = INBOX_ITEMS.filter((n) => n.unread && !inboxRead.has(n.id)).length
  /* DISTINCT open PRs, like the real ReviewsCountBadge (a batch PR linked to
     several issues counts once). */
  const reviewCount = new Set(REVIEWS.map((r) => r.prNumber)).size

  const go = (target: WebNav) => () => {
    setNav(target)
    closeIssue()
  }

  return (
    <div className="web-sidebar">
      <div className="web-team is-click">
        <span className="web-team-avatar">{PROJECT.name[0]}</span>
        <span className="web-team-name">{PROJECT.name}</span>
        <IcChevsUpDown size={14} className="ide-c-muted" />
      </div>

      <div className="web-side-scroll">
        <div className="web-nav">
          <NavItem icon={<IcSearch size={15} />} label="Search" />
          <NavItem
            icon={<IcCircleUser size={15} />}
            label="My Issues"
            active={nav === `my-issues`}
            onClick={go(`my-issues`)}
          />
          <NavItem
            icon={<IcInbox size={15} />}
            label="Inbox"
            active={nav === `inbox`}
            onClick={go(`inbox`)}
            trailing={unread > 0 ? <span className="web-nav-badge">{unread}</span> : undefined}
          />
          <NavItem
            icon={<IcGitPullRequest size={15} />}
            label="Reviews"
            trailing={
              reviewCount > 0 ? <span className="web-nav-badge">{reviewCount}</span> : undefined
            }
          />
          <NavItem
            icon={<IcBot size={15} />}
            label="Agents"
            trailing={AGENTS_RUNNING > 0 ? <span className="web-nav-dot" /> : undefined}
          />
          <NavItem
            icon={<IcLifeBuoy size={15} />}
            label="Support"
            active={nav === `support`}
            onClick={go(`support`)}
          />
        </div>

        <div className="web-group">
          <div className="web-group-head">
            <span className="web-group-label">Projects</span>
            <button className="web-group-plus" type="button" title="Create project">
              <IcPlus size={14} />
            </button>
          </div>
          {WEB_PROJECTS.map((project, i) => {
            const Icon = projectIcon[project.icon]
            /* The demo board is the first (dogfood) project. */
            const isBoard = i === 0
            return (
              <NavItem
                key={project.slug}
                icon={<Icon size={15} style={{ color: project.color }} />}
                label={project.name}
                active={nav === `project` && isBoard}
                onClick={isBoard ? go(`project`) : undefined}
                trailing={
                  project.isPublic ? (
                    <IcGlobe size={13} className="ide-c-muted" />
                  ) : undefined
                }
              />
            )
          })}
        </div>
      </div>

      <div className="web-side-footer">
        <NavItem icon={<IcSparkles size={15} />} label="Getting started" />
        <NavItem icon={<IcMegaphone size={15} />} label="Send feedback" />
        <div className="web-user is-click">
          <span className="web-user-avatar">{WEB_USER.initials}</span>
          <span className="web-user-email">{WEB_USER.email}</span>
          <IcChevsUpDown size={14} className="ide-c-muted" />
        </div>
      </div>
    </div>
  )
}
