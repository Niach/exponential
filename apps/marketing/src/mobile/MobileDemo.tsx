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
  IcCompose,
  IcGitMerge,
  IcGitPr,
  IcInbox,
  IcInfo,
  IcLifeBuoy,
  IcListFilter,
  IcListTodo,
  IcLucideCircleDashed,
  IcMessage,
  IcMinus,
  IcMonitor,
  IcMore,
  IcSearch,
  IcSend,
  IcSettings,
  IcSignalHigh,
  IcSignalMedium,
  IcSparkles,
  IcSquare,
  IcUserPlus,
  IcWrench,
} from "../components/icons"
import {
  mobAgents,
  mobAssigned,
  mobDesktops,
  mobDetailIssue,
  mobInboxItems,
  mobProjects,
  mobSearchQuery,
  mobSearchResults,
  mobSteerDiff,
  mobSteerFeed,
  type MobAgentState,
  type MobInboxType,
  type MobIssue,
  type MobPriority,
  type MobStatus,
} from "./data"

/* ─── Small glyph helpers ─── */

/* CSS custom properties resolve inside inline styles, so these reference
   tokens.css directly — the single palette source. */
const statusColor: Record<MobStatus, string> = {
  in_progress: `var(--st-progress)`,
  in_review: `var(--st-review)`,
  backlog: `var(--fg-muted)`,
  done: `var(--st-done)`,
}

/* The native status glyph set: pie-clock wedges for the started statuses
   (icons.json progress-2-4 / progress-3-4) and a FILLED disc for Done. */
const PieGlyph = ({
  size,
  wedge,
  style,
}: {
  size: number
  wedge: string
  style?: React.CSSProperties
}) => (
  <svg
    width={size}
    height={size}
    viewBox={`0 0 24 24`}
    fill={`none`}
    stroke={`currentColor`}
    strokeWidth={2}
    style={{ display: `block`, flexShrink: 0, ...style }}
    aria-hidden
  >
    <circle cx={12} cy={12} r={10} />
    <path d={wedge} fill={`currentColor`} stroke={`none`} />
  </svg>
)

const DoneGlyph = ({
  size,
  style,
}: {
  size: number
  style?: React.CSSProperties
}) => (
  <svg
    width={size}
    height={size}
    viewBox={`0 0 24 24`}
    style={{ display: `block`, flexShrink: 0, ...style }}
    aria-hidden
  >
    <circle cx={12} cy={12} r={10} fill={`currentColor`} />
    <path
      d={`m8.5 12 2.5 2.5 5-5`}
      fill={`none`}
      stroke={`#0b0b0e`}
      strokeWidth={2}
      strokeLinecap={`round`}
      strokeLinejoin={`round`}
    />
  </svg>
)

const StatusIcon = ({
  status,
  size = 15,
}: {
  status: MobStatus
  size?: number
}) => {
  const style = { color: statusColor[status] }
  if (status === `in_progress`)
    return (
      <PieGlyph
        size={size}
        wedge={`M12 12 L12 6 A6 6 0 0 1 12 18 Z`}
        style={style}
      />
    )
  if (status === `in_review`)
    return (
      <PieGlyph
        size={size}
        wedge={`M12 12 L12 6 A6 6 0 1 1 6 12 Z`}
        style={style}
      />
    )
  if (status === `done`) return <DoneGlyph size={size} style={style} />
  if (status === `backlog`)
    return <IcLucideCircleDashed size={size} style={style} />
  return <IcCircle size={size} style={style} />
}

const PriorityIcon = ({ priority }: { priority: MobPriority }) => {
  if (priority === `high`)
    return <IcSignalHigh size={15} style={{ color: `var(--pr-high)` }} />
  if (priority === `medium`)
    return <IcSignalMedium size={15} style={{ color: `var(--st-progress)` }} />
  return <IcMinus size={15} style={{ color: `var(--fg-dim)` }} />
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
    <path
      d={`M27.2 4.4 v4.2 a2.1 2.1 0 0 0 0 -4.2 z`}
      fill={`rgba(255,255,255,0.35)`}
    />
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

const Avatar = ({
  initials,
  size = 22,
}: {
  initials: string
  size?: number
}) => (
  <span
    className={`mob-avatar`}
    style={{ width: size, height: size, fontSize: size * 0.42 }}
  >
    {initials}
  </span>
)

/* ─── Tabs / tour plumbing ───
   The native MobileTabBar order: Issues · My Work · Support · Agents ·
   Reviews · Search (icon-only) + the detached compose FAB; Support and
   Reviews render for fidelity but stay inert — the demo keeps only the
   tabs that carry a full recreation. `steer` is the Agents sub-screen. */

type MobTab = `issues` | `mywork` | `agents` | `steer` | `search`

const TOUR: { tab: MobTab }[] = [
  { tab: `issues` },
  { tab: `agents` },
  { tab: `steer` },
  { tab: `mywork` },
]

/* ─── Shared rows ─── */

const IssueRow = ({ issue }: { issue: MobIssue }) => (
  <div className={`mob-row`}>
    <PriorityIcon priority={issue.priority} />
    <span className={`mob-row-id`}>{issue.identifier}</span>
    <StatusIcon status={issue.status} />
    <span className={`mob-row-title`}>{issue.title}</span>
    {issue.label ? (
      <span
        className={`mob-row-label-dot`}
        style={{ background: issue.label.color }}
        title={issue.label.name}
      />
    ) : null}
    {issue.assignee ? <Avatar initials={issue.assignee} size={20} /> : null}
    <IcChev size={15} className={`mob-row-chev`} />
  </div>
)

/* ─── Bottom dock (6 icon-only tabs + compose FAB) ─── */

const DockBtn = ({
  active,
  onClick,
  dot,
  label,
  children,
}: {
  active: boolean
  onClick: () => void
  dot?: `green` | `unread`
  label: string
  children: React.ReactNode
}) => (
  <button
    type={`button`}
    className={active ? `mob-dock-btn mob-dock-btn-active` : `mob-dock-btn`}
    onClick={onClick}
    tabIndex={-1}
    aria-label={label}
  >
    {children}
    {dot ? (
      <span
        className={
          dot === `green` ? `mob-dock-dot mob-dock-dot-green` : `mob-dock-dot`
        }
      />
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
      <DockBtn
        active={tab === `issues`}
        onClick={() => onTab(`issues`)}
        label={`Issues`}
      >
        <IcListTodo size={18} />
      </DockBtn>
      <DockBtn
        active={tab === `mywork`}
        onClick={() => onTab(`mywork`)}
        dot={inboxUnread ? `unread` : undefined}
        label={`My Work`}
      >
        <IcInbox size={18} />
      </DockBtn>
      <DockBtn active={false} onClick={() => {}} label={`Support`}>
        <IcLifeBuoy size={18} />
      </DockBtn>
      <DockBtn
        active={tab === `agents` || tab === `steer`}
        onClick={() => onTab(`agents`)}
        dot={`green`}
        label={`Agents`}
      >
        <IcBot size={18} />
      </DockBtn>
      <DockBtn active={false} onClick={() => {}} label={`Reviews`}>
        <IcGitPr size={18} />
      </DockBtn>
      <DockBtn
        active={tab === `search`}
        onClick={() => onTab(`search`)}
        label={`Search`}
      >
        <IcSearch size={18} />
      </DockBtn>
    </div>
    <div className={`mob-fab`}>
      <IcCompose size={20} />
    </div>
  </div>
)

/* ─── Issues tab — board list under the real nav bar: centered board-name
   combobox, filter + settings sharing ONE trailing capsule (filters live
   behind the funnel — the app has no filter-chip strip). ─── */

const IssuesScreen = ({
  reduce,
  projIdx,
  cycleProject,
}: {
  reduce: boolean
  projIdx: number
  cycleProject: () => void
}) => {
  const project = mobProjects[projIdx]

  return (
    <>
      <div className={`mob-nav`}>
        <button
          type={`button`}
          className={`mob-nav-board`}
          onClick={cycleProject}
          tabIndex={-1}
        >
          {project.name}
          <IcChevSwap size={13} />
        </button>
        <span className={`mob-navcaps`}>
          <IcListFilter size={15} />
          <IcSettings size={15} />
        </span>
      </div>
      <motion.div
        key={projIdx}
        className={`mob-list`}
        initial={reduce ? false : { opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.28, ease: `easeOut` }}
      >
        {project.groups.map((group) => (
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

/* ─── Agents tab — online desktops (remote Start coding) + running
   sessions with their coding-session state lines ─── */

const AGENT_STATE: Record<
  MobAgentState,
  { label: string; tone: `green` | `amber` }
> = {
  live: { label: `Live`, tone: `green` },
  needs_input: { label: `Needs input`, tone: `amber` },
  ready: { label: `Ready for review`, tone: `green` },
}

const AgentsScreen = ({ onOpenSteer }: { onOpenSteer: () => void }) => (
  <>
    <h2 className={`mob-title is-center`}>Agents</h2>
    <div className={`mob-list mob-list-scrollpad`}>
      <div className={`mob-section-head`}>My desktops</div>
      {mobDesktops.map((device) => (
        <div key={device} className={`mob-row mob-desktop-row`}>
          <IcMonitor size={15} className={`mob-desktop-icon`} />
          <span className={`mob-desktop-name`}>{device}</span>
          <span className={`mob-startpill`}>Start coding</span>
        </div>
      ))}
      <div className={`mob-section-head`}>Running</div>
      {mobAgents.map((agent, i) => {
        const state = AGENT_STATE[agent.state]
        return (
          <button
            key={agent.identifier}
            type={`button`}
            className={`mob-row mob-agent-row`}
            onClick={i === 0 ? onOpenSteer : undefined}
            tabIndex={-1}
          >
            <span
              className={
                state.tone === `amber`
                  ? `mob-agent-dot is-amber`
                  : `mob-agent-dot`
              }
            />
            <span className={`mob-agent-main`}>
              <span className={`mob-agent-line1`}>
                <span className={`mob-row-id`}>{agent.identifier}</span>
                <span className={`mob-agent-title`}>{agent.title}</span>
              </span>
              <span className={`mob-agent-meta`}>
                <span
                  className={
                    state.tone === `amber`
                      ? `mob-agent-state is-amber`
                      : `mob-agent-state`
                  }
                >
                  {state.label}
                </span>
                {` ${agent.device}`}
              </span>
            </span>
            <IcInfo size={15} className={`mob-row-chev`} />
          </button>
        )
      })}
    </div>
  </>
)

/* ─── Live steer viewer — the native AgentSessionView: "Live · <device>"
   header (no issue title up there), sparkles narration WITHOUT bubbles,
   wrench tool rows, pinned "Latest changes" chip, message composer.
   No terminal rendering on mobile or web. ─── */

const SteerScreen = ({ onBack }: { onBack: () => void }) => (
  <>
    <div className={`mob-header`}>
      <button
        type={`button`}
        className={`mob-backbtn`}
        onClick={onBack}
        tabIndex={-1}
        aria-label={`Back`}
      >
        <IcChevLeft size={19} stroke={2.2} />
      </button>
      <span className={`mob-steer-title`}>
        <span className={`mob-agent-dot`} />
        <span className={`mob-steer-device`}>Live · dennis-mbp.local</span>
      </span>
      <span className={`mob-stopbtn`}>
        <IcSquare size={14} />
      </span>
    </div>
    <div className={`mob-feed`}>
      {mobSteerFeed.map((item, i) =>
        item.kind === `narration` ? (
          <div key={i} className={`mob-feed-narr`}>
            <IcSparkles size={12} />
            <span>{item.text}</span>
          </div>
        ) : (
          <div key={i} className={`mob-feed-tool`}>
            <IcWrench size={12} />
            <span className={`mob-feed-tool-name`}>{item.name}</span>
            <span className={`mob-feed-tool-detail`}>{item.detail}</span>
          </div>
        )
      )}
      <div className={`mob-feed-typing`}>
        <span className={`mob-agent-dot`} />
        Claude is working…
      </div>
    </div>
    <div className={`mob-steer-input`}>
      <div className={`mob-diffchip`}>
        <IcGitMerge size={13} />
        <span className={`mob-diffchip-label`}>Latest changes</span>
        <span className={`mob-diffchip-stats`}>
          {`${mobSteerDiff.files} file`}
          <span className={`mob-diff-add`}>{` +${mobSteerDiff.add}`}</span>
          <span className={`mob-diff-del`}>{` −${mobSteerDiff.del}`}</span>
        </span>
        <IcChev size={14} className={`mob-row-chev`} />
      </div>
      <div className={`mob-steer-fieldrow`}>
        <span className={`mob-steer-field`}>Message the agent…</span>
        <span className={`mob-composer-send`}>
          <IcSend size={15} />
        </span>
      </div>
    </div>
  </>
)

/* ─── My Work tab — Inbox + My Issues merged (native MobileTabBar) ─── */

const inboxIcon = (type: MobInboxType) => {
  if (type === `pr_opened`) return <IcGitPr size={14} />
  if (type === `pr_merged`) return <IcGitMerge size={14} />
  if (type === `assigned`) return <IcUserPlus size={14} />
  return <IcMessage size={14} />
}

const MyWorkScreen = () => {
  const [seg, setSeg] = useState<`inbox` | `issues`>(`inbox`)
  return (
    <>
      <h2 className={`mob-title is-center`}>My Work</h2>
      <div className={`mob-segment`}>
        {(
          [
            [`inbox`, `Inbox`],
            [`issues`, `My Issues`],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type={`button`}
            className={seg === id ? `mob-segbtn is-active` : `mob-segbtn`}
            onClick={() => setSeg(id)}
            tabIndex={-1}
          >
            {label}
          </button>
        ))}
      </div>
      <div className={`mob-list mob-list-scrollpad`}>
        {seg === `inbox`
          ? mobInboxItems.map((n) => (
              <div
                key={n.identifier}
                className={
                  n.unread ? `mob-inbox-row` : `mob-inbox-row mob-inbox-read`
                }
              >
                <span className={`mob-inbox-badge`}>{inboxIcon(n.type)}</span>
                <span className={`mob-inbox-main`}>
                  <span className={`mob-inbox-line1`}>
                    <span className={`mob-row-id`}>{n.identifier}</span>
                    <span
                      className={
                        n.unread
                          ? `mob-inbox-title mob-inbox-unread`
                          : `mob-inbox-title`
                      }
                    >
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
            ))
          : mobAssigned.map((issue) => (
              <IssueRow key={issue.identifier} issue={issue} />
            ))}
      </div>
    </>
  )
}

/* ─── Issue detail screen (static, used by docs embeds) ─── */

const IssueScreen = () => {
  const issue = mobDetailIssue
  return (
    <>
      <div className={`mob-header`}>
        <span className={`mob-backbtn`}>
          <IcChevLeft size={19} stroke={2.2} />
        </span>
        <span className={`mob-header-title`}>Issue</span>
        <span className={`mob-backbtn mob-morebtn`}>
          <IcMore size={17} />
        </span>
      </div>
      <span className={`mob-idpill`}>{issue.identifier}</span>
      <h2 className={`mob-title mob-title-issue`}>{issue.title}</h2>
      {/* the property CHIP BOX (wrapping capsules), like the app */}
      <div className={`mob-chipbox`}>
        <span className={`mob-propchip`}>
          <StatusIcon status={issue.statusKey} size={13} />
          {issue.status}
        </span>
        <span className={`mob-propchip`}>
          <IcSignalHigh size={13} style={{ color: `var(--pr-high)` }} />
          {issue.priority}
        </span>
        <span className={`mob-propchip`}>
          <Avatar initials={issue.assignee.initials} size={16} />
          {issue.assignee.name}
        </span>
        <span className={`mob-propchip`}>+</span>
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

/* ─── Interactive app shell (4-tab dock + FAB, tour) ─── */

const AppShell = ({
  reduce,
  autoTour,
}: {
  reduce: boolean
  autoTour: boolean
}) => {
  const [tab, setTab] = useState<MobTab>(`issues`)
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
      if (next.tab === `mywork`) setInboxSeen(true)
    }, 4200)
    return () => clearInterval(timer)
  }, [touring])

  const goto = (t: MobTab) => {
    setTab(t)
    if (t === `mywork`) setInboxSeen(true)
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
          <MyWorkScreen />
        )}
      </motion.div>
      {tab !== `steer` && (
        <BottomBar tab={tab} inboxUnread={!inboxSeen} onTab={goto} />
      )}
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
        {screen === `list` ? (
          <AppShell reduce={reduce} autoTour={autoTour} />
        ) : (
          <IssueScreen />
        )}
        <span className={`mob-home-indicator`} />
      </div>
    </motion.div>
  )
}
