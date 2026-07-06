import { useEffect, useRef, useState } from "react"
import { motion, useReducedMotion } from "motion/react"
import { Wifi } from "lucide-react"
import {
  IcBot,
  IcChev,
  IcChevDown,
  IcChevLeft,
  IcChevSwap,
  IcCircle,
  IcCircleCheck,
  IcCompose,
  IcEye,
  IcFilter,
  IcGitMerge,
  IcGitPr,
  IcInbox,
  IcKeyboard,
  IcListTodo,
  IcLucideCircleDashed,
  IcMessage,
  IcMinus,
  IcSearch,
  IcSignalHigh,
  IcSignalMedium,
  IcTimer,
  IcUserPlus,
} from "../components/icons"
import {
  mobAgents,
  mobAssigned,
  mobDetailIssue,
  mobInboxItems,
  mobProjects,
  mobSearchQuery,
  mobSearchResults,
  mobSteerLines,
  type MobInboxType,
  type MobIssue,
  type MobPriority,
  type MobStatus,
} from "./data"

/* ─── Small glyph helpers ─── */

const statusColor: Record<MobStatus, string> = {
  in_progress: `#facc15`,
  todo: `#fafafa`,
  backlog: `#a1a1a1`,
  done: `#22c55e`,
}

const StatusIcon = ({ status, size = 15 }: { status: MobStatus; size?: number }) => {
  const style = { color: statusColor[status] }
  if (status === `in_progress`) return <IcTimer size={size} style={style} />
  if (status === `done`) return <IcCircleCheck size={size} style={style} />
  if (status === `backlog`) return <IcLucideCircleDashed size={size} style={style} />
  return <IcCircle size={size} style={style} />
}

const PriorityIcon = ({ priority }: { priority: MobPriority }) => {
  if (priority === `high`) return <IcSignalHigh size={15} style={{ color: `#f97316` }} />
  if (priority === `medium`) return <IcSignalMedium size={15} style={{ color: `#facc15` }} />
  return <IcMinus size={15} style={{ color: `#5b5b60` }} />
}

const BatteryGlyph = () => (
  <svg width={30} height={13} viewBox={`0 0 30 13`} aria-hidden>
    <rect
      x={0.5}
      y={0.5}
      width={25}
      height={12}
      rx={3.5}
      fill={`none`}
      stroke={`rgba(255,255,255,0.35)`}
    />
    <rect x={2} y={2} width={21.6} height={9} rx={2.2} fill={`#fafafa`} />
    <text
      x={12.6}
      y={9.3}
      textAnchor={`middle`}
      fontSize={8}
      fontWeight={700}
      fill={`#0a0a0a`}
      fontFamily={`Inter, system-ui, sans-serif`}
    >
      96
    </text>
    <path d={`M27.2 4.4 v4.2 a2.1 2.1 0 0 0 0 -4.2 z`} fill={`rgba(255,255,255,0.35)`} />
  </svg>
)

const StatusBar = () => (
  <div className={`mob-statusbar`}>
    <span className={`mob-statusbar-time`}>20:22</span>
    <div className={`mob-island`} />
    <span className={`mob-statusbar-right`}>
      <Wifi size={15} strokeWidth={2.2} />
      <BatteryGlyph />
    </span>
  </div>
)

const Avatar = ({ initials, size = 22 }: { initials: string; size?: number }) => (
  <span
    className={`mob-avatar`}
    style={{ width: size, height: size, fontSize: size * 0.42 }}
  >
    {initials}
  </span>
)

/* ─── Tabs / tour plumbing ─── */

type MobTab = `issues` | `search` | `agents` | `steer` | `inbox`

const TOUR: { tab: MobTab; chip?: number }[] = [
  { tab: `issues`, chip: 0 },
  { tab: `issues`, chip: 1 },
  { tab: `issues`, chip: 2 },
  { tab: `agents` },
  { tab: `steer` },
  { tab: `inbox` },
]

/* ─── Shared rows ─── */

const IssueRow = ({ issue }: { issue: MobIssue }) => (
  <div className={`mob-row`}>
    <PriorityIcon priority={issue.priority} />
    <span className={`mob-row-id`}>{issue.identifier}</span>
    <StatusIcon status={issue.status} />
    <span className={`mob-row-title`}>{issue.title}</span>
    {issue.label ? (
      <span className={`mob-row-label`}>
        <span className={`mob-row-label-dot`} style={{ background: issue.label.color }} />
        {issue.label.name}
      </span>
    ) : null}
    {issue.assignee ? <Avatar initials={issue.assignee} size={20} /> : null}
    <IcChev size={15} className={`mob-row-chev`} />
  </div>
)

/* ─── Bottom dock (4 tabs + compose FAB) ─── */

const DockBtn = ({
  active,
  onClick,
  dot,
  children,
}: {
  active: boolean
  onClick: () => void
  dot?: `green` | `unread`
  children: React.ReactNode
}) => (
  <button
    type={`button`}
    className={active ? `mob-dock-btn mob-dock-btn-active` : `mob-dock-btn`}
    onClick={onClick}
    tabIndex={-1}
  >
    {children}
    {dot ? (
      <span className={dot === `green` ? `mob-dock-dot mob-dock-dot-green` : `mob-dock-dot`} />
    ) : null}
  </button>
)

const BottomBar = ({
  tab,
  inboxUnread,
  onTab,
}: {
  tab: MobTab
  inboxUnread: boolean
  onTab: (t: MobTab) => void
}) => (
  <div className={`mob-bottombar`}>
    <div className={`mob-dock`}>
      <DockBtn active={tab === `issues`} onClick={() => onTab(`issues`)}>
        <IcListTodo size={19} />
      </DockBtn>
      <DockBtn active={tab === `search`} onClick={() => onTab(`search`)}>
        <IcSearch size={19} />
      </DockBtn>
      <DockBtn
        active={tab === `agents` || tab === `steer`}
        onClick={() => onTab(`agents`)}
        dot={`green`}
      >
        <IcBot size={19} />
      </DockBtn>
      <DockBtn
        active={tab === `inbox`}
        onClick={() => onTab(`inbox`)}
        dot={inboxUnread ? `unread` : undefined}
      >
        <IcInbox size={19} />
      </DockBtn>
    </div>
    <div className={`mob-fab`}>
      <IcCompose size={20} />
    </div>
  </div>
)

/* ─── Issues tab — current project list + inline project switcher ─── */

const chips = [`All Issues`, `Active`, `Backlog`] as const

const chipStatuses: Record<number, MobStatus[]> = {
  0: [`in_progress`, `todo`, `backlog`, `done`],
  1: [`in_progress`, `todo`],
  2: [`backlog`],
}

const IssuesScreen = ({
  reduce,
  chip,
  setChip,
  projIdx,
  cycleProject,
}: {
  reduce: boolean
  chip: number
  setChip: (i: number) => void
  projIdx: number
  cycleProject: () => void
}) => {
  const project = mobProjects[projIdx]
  const visible = project.groups.filter((g) => chipStatuses[chip].includes(g.status))

  return (
    <>
      <button type={`button`} className={`mob-titlerow`} onClick={cycleProject} tabIndex={-1}>
        <h2 className={`mob-title`}>{project.name}</h2>
        <span className={`mob-title-switch`}>
          <IcChevSwap size={15} />
        </span>
      </button>
      <div className={`mob-divider`} />
      <div className={`mob-chips`}>
        <span className={`mob-chip-filter`}>
          <IcFilter size={14} />
        </span>
        {chips.map((label, i) => (
          <button
            key={label}
            type={`button`}
            className={i === chip ? `mob-chip mob-chip-active` : `mob-chip`}
            onClick={() => setChip(i)}
            tabIndex={-1}
          >
            {label}
          </button>
        ))}
      </div>
      <motion.div
        key={`${projIdx}-${chip}`}
        className={`mob-list`}
        initial={reduce ? false : { opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.28, ease: `easeOut` }}
      >
        {visible.map((group) => (
          <div key={group.status} className={`mob-group`}>
            <div className={`mob-group-head`}>
              <IcChevDown size={13} className={`mob-group-chev`} />
              <StatusIcon status={group.status} size={14} />
              <span className={`mob-group-label`}>{group.label}</span>
              <span className={`mob-group-count`}>{group.issues.length}</span>
            </div>
            {group.issues.map((issue) => (
              <IssueRow key={issue.identifier} issue={issue} />
            ))}
          </div>
        ))}
      </motion.div>
    </>
  )
}

/* ─── Search tab — cross-project search + assigned-to-you block ─── */

const SearchScreen = () => (
  <>
    <h2 className={`mob-title`}>Search</h2>
    <div className={`mob-search mob-search-live`}>
      <IcSearch size={16} />
      <span className={`mob-search-query`}>{mobSearchQuery}</span>
      <span className={`mob-caret`} />
    </div>
    <div className={`mob-list mob-list-scrollpad`}>
      <div className={`mob-section-head`}>Exponential</div>
      {mobSearchResults.map((issue) => (
        <IssueRow key={issue.identifier} issue={issue} />
      ))}
      <div className={`mob-section-head`}>Assigned to you</div>
      {mobAssigned.map((issue) => (
        <IssueRow key={issue.identifier} issue={issue} />
      ))}
    </div>
  </>
)

/* ─── Agents tab — running coding sessions ─── */

const AgentsScreen = ({ onOpenSteer }: { onOpenSteer: () => void }) => (
  <>
    <h2 className={`mob-title`}>Agents</h2>
    <div className={`mob-list mob-list-scrollpad`}>
      {mobAgents.map((agent, i) => (
        <button
          key={agent.identifier}
          type={`button`}
          className={`mob-row mob-agent-row`}
          onClick={i === 0 ? onOpenSteer : undefined}
          tabIndex={-1}
        >
          <span className={`mob-agent-dot`} />
          <span className={`mob-row-id`}>{agent.identifier}</span>
          <span className={`mob-agent-main`}>
            <span className={`mob-agent-title`}>{agent.title}</span>
            <span className={`mob-agent-meta`}>{agent.meta}</span>
          </span>
          <IcChev size={15} className={`mob-row-chev`} />
        </button>
      ))}
    </div>
  </>
)

/* ─── Live steer viewer — static terminal snapshot ─── */

const SteerScreen = ({ onBack }: { onBack: () => void }) => (
  <>
    <div className={`mob-header`}>
      <button type={`button`} className={`mob-backbtn`} onClick={onBack} tabIndex={-1}>
        <IcChevLeft size={19} stroke={2.2} />
      </button>
      <span className={`mob-steer-title`}>
        <span className={`mob-header-id`}>EXP-12</span>
        <span className={`mob-live-pill`}>
          <span className={`mob-agent-dot`} />
          Live
        </span>
      </span>
      <span className={`mob-steer-presence`}>
        <span className={`mob-presence-chip`}>
          <IcEye size={13} /> 2
        </span>
        <span className={`mob-presence-chip`}>
          <IcKeyboard size={13} />
        </span>
      </span>
    </div>
    <div className={`mob-steer-term`}>
      {mobSteerLines.map((line, i) => (
        <div key={i} className={`mob-steer-line`}>
          {line.kind === `cmd` && <span className={`mob-steer-prompt`}>{`$ `}</span>}
          {line.kind === `ok` && <span className={`mob-steer-ok`}>{`✓ `}</span>}
          {line.kind === `claude` && <span className={`mob-steer-claude`}>{`● `}</span>}
          <span className={line.kind === `cmd` ? `mob-steer-cmd` : `mob-steer-out`}>
            {line.text}
          </span>
        </div>
      ))}
      <div className={`mob-steer-line`}>
        <span className={`mob-caret mob-caret-term`} />
      </div>
    </div>
    <div className={`mob-steer-input`}>
      <div className={`mob-steer-keys`}>
        {[`esc`, `^C`, `tab`, `↑`, `↓`].map((k) => (
          <span key={k} className={`mob-steer-key`}>
            {k}
          </span>
        ))}
      </div>
      <div className={`mob-steer-fieldrow`}>
        <span className={`mob-steer-field`}>Type to steer…</span>
        <span className={`mob-takecontrol`}>Take control</span>
      </div>
    </div>
  </>
)

/* ─── Inbox tab — single Linear-style activity stream ─── */

const inboxIcon = (type: MobInboxType) => {
  if (type === `pr_opened`) return <IcGitPr size={14} />
  if (type === `pr_merged`) return <IcGitMerge size={14} />
  if (type === `assigned`) return <IcUserPlus size={14} />
  return <IcMessage size={14} />
}

const InboxScreen = () => (
  <>
    <h2 className={`mob-title`}>Inbox</h2>
    <div className={`mob-list mob-list-scrollpad`}>
      {mobInboxItems.map((n) => (
        <div
          key={n.identifier}
          className={n.unread ? `mob-inbox-row` : `mob-inbox-row mob-inbox-read`}
        >
          <span className={`mob-inbox-badge`}>{inboxIcon(n.type)}</span>
          <span className={`mob-inbox-main`}>
            <span className={`mob-inbox-line1`}>
              <span className={`mob-row-id`}>{n.identifier}</span>
              <span className={n.unread ? `mob-inbox-title mob-inbox-unread` : `mob-inbox-title`}>
                {n.title}
              </span>
            </span>
            <span className={`mob-inbox-sentence`}>{n.sentence}</span>
          </span>
          <span className={`mob-inbox-meta`}>
            <span className={`mob-inbox-time`}>{n.time}</span>
            {n.unread ? <span className={`mob-inbox-dot`} /> : null}
          </span>
        </div>
      ))}
    </div>
  </>
)

/* ─── Issue detail screen (static, used by docs embeds) ─── */

const IssueScreen = () => {
  const issue = mobDetailIssue
  return (
    <>
      <div className={`mob-header`}>
        <span className={`mob-backbtn`}>
          <IcChevLeft size={19} stroke={2.2} />
        </span>
        <span className={`mob-header-id`}>{issue.identifier}</span>
      </div>
      <h2 className={`mob-title mob-title-issue`}>{issue.title}</h2>
      <div className={`mob-card mob-props`}>
        <div className={`mob-prop-row`}>
          <span className={`mob-prop-key`}>Status</span>
          <span className={`mob-prop-value`}>
            <IcTimer size={15} style={{ color: `#facc15` }} />
            {issue.status}
          </span>
        </div>
        <div className={`mob-prop-row`}>
          <span className={`mob-prop-key`}>Priority</span>
          <span className={`mob-prop-value`}>
            <IcSignalHigh size={15} style={{ color: `#f97316` }} />
            {issue.priority}
          </span>
        </div>
        <div className={`mob-prop-row`}>
          <span className={`mob-prop-key`}>Assignee</span>
          <span className={`mob-prop-value`}>
            <Avatar initials={issue.assignee.initials} size={19} />
            {issue.assignee.name}
          </span>
        </div>
      </div>
      <div className={`mob-desc`}>
        {issue.description.map((p) => (
          <p key={p}>{p}</p>
        ))}
      </div>
      <div className={`mob-activity`}>
        <span className={`mob-activity-head`}>Activity</span>
        <div className={`mob-event`}>
          <span className={`mob-event-dot`} />
          {issue.event}
        </div>
        <div className={`mob-card mob-comment`}>
          <div className={`mob-comment-meta`}>
            <Avatar initials={issue.comment.initials} size={20} />
            <span className={`mob-comment-author`}>{issue.comment.author}</span>
            <span className={`mob-comment-time`}>{issue.comment.time}</span>
          </div>
          <p className={`mob-comment-body`}>{issue.comment.body}</p>
        </div>
      </div>
    </>
  )
}

/* ─── Interactive app shell (4-tab dock, tour) ─── */

const AppShell = ({ reduce, autoTour }: { reduce: boolean; autoTour: boolean }) => {
  const [tab, setTab] = useState<MobTab>(`issues`)
  const [chip, setChip] = useState(0)
  const [projIdx, setProjIdx] = useState(0)
  const [inboxSeen, setInboxSeen] = useState(false)
  const [touring, setTouring] = useState(autoTour && !reduce)
  const step = useRef(0)

  useEffect(() => {
    if (!touring) return
    const timer = setInterval(() => {
      step.current = (step.current + 1) % TOUR.length
      const next = TOUR[step.current]
      setTab(next.tab)
      if (next.chip !== undefined) setChip(next.chip)
      if (next.tab === `inbox`) setInboxSeen(true)
    }, 4200)
    return () => clearInterval(timer)
  }, [touring])

  const goto = (t: MobTab) => {
    setTab(t)
    if (t === `inbox`) setInboxSeen(true)
  }

  return (
    <div
      className={`mob-app`}
      onPointerDown={touring ? () => setTouring(false) : undefined}
    >
      <motion.div
        key={tab}
        className={`mob-app-pane`}
        initial={reduce ? false : { opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.28, ease: `easeOut` }}
      >
        {tab === `issues` ? (
          <IssuesScreen
            reduce={reduce}
            chip={chip}
            setChip={setChip}
            projIdx={projIdx}
            cycleProject={() => setProjIdx((i) => (i + 1) % mobProjects.length)}
          />
        ) : tab === `search` ? (
          <SearchScreen />
        ) : tab === `agents` ? (
          <AgentsScreen onOpenSteer={() => goto(`steer`)} />
        ) : tab === `steer` ? (
          <SteerScreen onBack={() => goto(`agents`)} />
        ) : (
          <InboxScreen />
        )}
      </motion.div>
      {tab !== `steer` && <BottomBar tab={tab} inboxUnread={!inboxSeen} onTab={goto} />}
    </div>
  )
}

/* ─── Phone frame ─── */

export const MobileDemo = ({
  screen = `list`,
  autoTour = false,
  className,
}: {
  screen?: `list` | `issue`
  autoTour?: boolean
  className?: string
}) => {
  const reduce = useReducedMotion() ?? false
  return (
    <motion.div
      className={className ? `mob-frame ${className}` : `mob-frame`}
      initial={reduce ? false : { opacity: 0, y: 28 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, amount: 0.25 }}
      transition={{ duration: 0.6, ease: `easeOut` }}
    >
      <div className={`mob-screen`}>
        <StatusBar />
        {screen === `list` ? <AppShell reduce={reduce} autoTour={autoTour} /> : <IssueScreen />}
        <span className={`mob-home-indicator`} />
      </div>
    </motion.div>
  )
}
