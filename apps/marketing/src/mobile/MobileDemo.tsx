import { useEffect, useState } from "react"
import { motion, useReducedMotion } from "motion/react"
import { Wifi } from "lucide-react"
import {
  IcChev,
  IcChevDown,
  IcChevLeft,
  IcCircle,
  IcCircleCheck,
  IcCompose,
  IcFilter,
  IcGrid,
  IcInbox,
  IcLucideCircleDashed,
  IcMinus,
  IcSearch,
  IcSignalHigh,
  IcSignalMedium,
  IcTimer,
  IcUser,
} from "../components/icons"
import {
  mobDetailIssue,
  mobGroups,
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

/* ─── Issue list screen ─── */

const chips = [`All Issues`, `Active`, `Backlog`] as const

const chipStatuses: Record<number, MobStatus[]> = {
  0: [`in_progress`, `todo`, `backlog`, `done`],
  1: [`in_progress`, `todo`],
  2: [`backlog`],
}

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

const ListScreen = ({ reduce }: { reduce: boolean }) => {
  const [chip, setChip] = useState(0)

  useEffect(() => {
    if (reduce) return
    const timer = setInterval(() => setChip((c) => (c + 1) % chips.length), 4200)
    return () => clearInterval(timer)
  }, [reduce])

  const visible = mobGroups.filter((g) => chipStatuses[chip].includes(g.status))

  return (
    <>
      <div className={`mob-header`}>
        <span className={`mob-backbtn`}>
          <IcChevLeft size={19} stroke={2.2} />
        </span>
      </div>
      <h2 className={`mob-title`}>Exponential</h2>
      <div className={`mob-search`}>
        <IcSearch size={16} />
        <span>Search issues</span>
      </div>
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
        key={chip}
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
      <div className={`mob-bottombar`}>
        <div className={`mob-dock`}>
          <IcGrid size={19} />
          <IcUser size={19} />
          <IcInbox size={19} />
        </div>
        <div className={`mob-fab`}>
          <IcCompose size={20} />
        </div>
      </div>
    </>
  )
}

/* ─── Issue detail screen ─── */

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

/* ─── Phone frame ─── */

export const MobileDemo = ({
  screen = `list`,
  className,
}: {
  screen?: `list` | `issue`
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
        {screen === `list` ? <ListScreen reduce={reduce} /> : <IssueScreen />}
        <span className={`mob-home-indicator`} />
      </div>
    </motion.div>
  )
}
