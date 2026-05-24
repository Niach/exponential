import { useEffect, useState } from "react"
import {
  AlertTriangle,
  Bell,
  Calendar,
  ChevronDown,
  CircleCheck,
  CircleDashed,
  CircleX,
  Circle as CircleIcon,
  Code2,
  ExternalLink,
  Hourglass,
  Minus,
  Plus,
  Search,
  SignalHigh,
  SignalLow,
  SignalMedium,
} from "lucide-react"

type StatusKey =
  | `backlog`
  | `todo`
  | `in_progress`
  | `done`
  | `cancelled`
type PriorityKey = `none` | `urgent` | `high` | `medium` | `low`

type LabelDot = { name: string; color: string }

type Issue = {
  id: string
  ident: string
  title: string
  status: StatusKey
  priority: PriorityKey
  due?: string
  labels?: LabelDot[]
  assignee: string
}

const STATUS: Record<
  StatusKey,
  { label: string; color: string; Icon: typeof CircleIcon }
> = {
  backlog: { label: `Backlog`, color: `oklch(0.708 0 0)`, Icon: CircleDashed },
  todo: { label: `Todo`, color: `oklch(0.85 0 0)`, Icon: CircleIcon },
  in_progress: {
    label: `In Progress`,
    color: `oklch(0.795 0.184 86.05)`,
    Icon: Hourglass,
  },
  done: {
    label: `Done`,
    color: `oklch(0.723 0.219 149.58)`,
    Icon: CircleCheck,
  },
  cancelled: {
    label: `Cancelled`,
    color: `oklch(0.637 0.237 25.33)`,
    Icon: CircleX,
  },
}

const PRIORITY: Record<
  PriorityKey,
  { color: string; Icon: typeof CircleIcon }
> = {
  none: { color: `oklch(0.708 0 0)`, Icon: Minus },
  urgent: { color: `oklch(0.637 0.237 25.33)`, Icon: AlertTriangle },
  high: { color: `oklch(0.705 0.213 47.6)`, Icon: SignalHigh },
  medium: { color: `oklch(0.795 0.184 86.05)`, Icon: SignalMedium },
  low: { color: `oklch(0.623 0.214 259.85)`, Icon: SignalLow },
}

const LABEL_FEATURE = `oklch(0.72 0.18 145)`
const LABEL_POLISH = `oklch(0.72 0.16 280)`
const LABEL_UX = `oklch(0.72 0.16 245)`
const LABEL_BUG = `oklch(0.637 0.237 25.33)`
const LABEL_INTEGRATION = `oklch(0.75 0.16 75)`

const seed: Issue[] = [
  {
    id: `m1`,
    ident: `EXP-24`,
    title: `Email digest of stale issues`,
    status: `todo`,
    priority: `urgent`,
    due: `May 2`,
    labels: [{ name: `feature`, color: LABEL_FEATURE }],
    assignee: `D`,
  },
  {
    id: `m2`,
    ident: `EXP-23`,
    title: `Bulk-edit selected issues`,
    status: `todo`,
    priority: `high`,
    labels: [{ name: `feature`, color: LABEL_FEATURE }],
    assignee: `N`,
  },
  {
    id: `m3`,
    ident: `EXP-22`,
    title: `Drag to reorder within a status group`,
    status: `todo`,
    priority: `high`,
    labels: [
      { name: `polish`, color: LABEL_POLISH },
      { name: `ux`, color: LABEL_UX },
    ],
    assignee: `D`,
  },
  {
    id: `m4`,
    ident: `EXP-21`,
    title: `Issue templates per project`,
    status: `todo`,
    priority: `medium`,
    labels: [{ name: `feature`, color: LABEL_FEATURE }],
    assignee: `N`,
  },
  {
    id: `m5`,
    ident: `EXP-19`,
    title: `GitHub PR linking via commit`,
    status: `todo`,
    priority: `low`,
    labels: [{ name: `integration`, color: LABEL_INTEGRATION }],
    assignee: `D`,
  },
  {
    id: `m6`,
    ident: `EXP-18`,
    title: `Crash when opening empty workspace`,
    status: `in_progress`,
    priority: `urgent`,
    labels: [{ name: `bug`, color: LABEL_BUG }],
    assignee: `D`,
  },
]

type Toast = { id: number; who: string; ident: string; text: string } | null

export function ProductMobile({ animate = true }: { animate?: boolean }) {
  const [issues, setIssues] = useState<Issue[]>(seed)
  const [flashId, setFlashId] = useState<string | null>(null)
  const [toast, setToast] = useState<Toast>(null)
  const [time, setTime] = useState(`9:41`)

  useEffect(() => {
    setTime(
      new Date().toLocaleTimeString(`en-US`, {
        hour: `numeric`,
        minute: `2-digit`,
      })
    )
  }, [])

  useEffect(() => {
    if (!animate) return
    let tick = 0
    let toastId = 0
    const cycle = () => {
      tick++
      const m = tick % 4
      if (m === 1) {
        toastId++
        setToast({
          id: toastId,
          who: `niach`,
          ident: `EXP-23`,
          text: `moved to In Progress`,
        })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `m2` ? { ...i, status: `in_progress` } : i))
          )
          setFlashId(`m2`)
          setTimeout(() => setFlashId(null), 1000)
        }, 500)
        setTimeout(() => setToast(null), 2800)
      } else if (m === 2) {
        toastId++
        setToast({
          id: toastId,
          who: `danny`,
          ident: `EXP-24`,
          text: `moved to In Progress`,
        })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `m1` ? { ...i, status: `in_progress` } : i))
          )
          setFlashId(`m1`)
          setTimeout(() => setFlashId(null), 1000)
        }, 500)
        setTimeout(() => setToast(null), 2800)
      } else if (m === 3) {
        toastId++
        setToast({
          id: toastId,
          who: `niach`,
          ident: `EXP-19`,
          text: `marked Done`,
        })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `m5` ? { ...i, status: `done` } : i))
          )
          setFlashId(`m5`)
          setTimeout(() => setFlashId(null), 1000)
        }, 500)
        setTimeout(() => setToast(null), 2800)
      } else {
        setIssues(seed)
        setToast(null)
      }
    }
    cycle()
    const id = setInterval(cycle, 3600)
    return () => clearInterval(id)
  }, [animate])

  const inProgress = issues.filter((i) => i.status === `in_progress`)
  const todo = issues.filter((i) => i.status === `todo`)
  const done = issues.filter((i) => i.status === `done`)
  const total = inProgress.length + todo.length + done.length

  return (
    <div className="phone" role="img" aria-label="Exponential iOS app">
      <div className="phone-frame">
        <div className="phone-screen">
          <div className="phone-statusbar">
            <span className="phone-time">{time}</span>
            <div className="phone-island" />
            <span className="phone-status-icons">
              <SignalHigh size={12} strokeWidth={2.4} />
              <span className="phone-battery">
                <span className="phone-battery-fill" />
              </span>
            </span>
          </div>

          <div className="m-app">
            <div className="m-toast-region" aria-live="polite">
              {toast && (
                <div key={toast.id} className="m-toast">
                  <span className="m-toast-icon">
                    <Bell size={13} strokeWidth={2} />
                  </span>
                  <div className="m-toast-body">
                    <strong>{toast.ident}</strong>
                    <span>
                      {toast.who} {toast.text}
                    </span>
                  </div>
                </div>
              )}
            </div>

            <div className="m-navbar">
              <div className="m-nav-row">
                <div className="m-nav-workspace">
                  <span className="m-ws-avatar">A</span>
                  <span>Acme</span>
                </div>
                <div className="m-nav-actions">
                  <button className="m-icon-btn" aria-label="Search">
                    <Search size={14} strokeWidth={2} />
                  </button>
                  <button className="m-icon-btn" aria-label="New issue">
                    <Plus size={15} strokeWidth={2.2} />
                  </button>
                </div>
              </div>
              <h2 className="m-title">Exponential</h2>
              <div className="m-subtitle">{total} issues</div>
            </div>

            <div className="m-repo-banner">
              <Code2 size={11} strokeWidth={2} />
              <span>Niach/exponential</span>
              <ExternalLink size={10} strokeWidth={2} />
            </div>

            <div className="m-tabs">
              <button className="m-tab is-active">All Issues</button>
              <button className="m-tab">Active</button>
              <button className="m-tab">Backlog</button>
            </div>

            <div className="m-list">
              {inProgress.length > 0 && (
                <MGroup
                  kind="in_progress"
                  count={inProgress.length}
                  issues={inProgress}
                  flashId={flashId}
                />
              )}
              {todo.length > 0 && (
                <MGroup
                  kind="todo"
                  count={todo.length === 5 ? 18 : 18 - (5 - todo.length)}
                  issues={todo}
                  flashId={flashId}
                />
              )}
              {done.length > 0 && (
                <MGroup
                  kind="done"
                  count={done.length}
                  issues={done}
                  flashId={flashId}
                />
              )}
            </div>

            <span className="m-home-indicator" aria-hidden />
          </div>
        </div>
      </div>
    </div>
  )
}

function MGroup({
  kind,
  count,
  issues,
  flashId,
}: {
  kind: StatusKey
  count: number
  issues: Issue[]
  flashId: string | null
}) {
  const cfg = STATUS[kind]
  const Sig = cfg.Icon
  return (
    <div className="m-group">
      <div className="m-group-head">
        <ChevronDown
          size={11}
          strokeWidth={2.2}
          style={{ color: `rgba(255,255,255,0.45)` }}
        />
        <Sig size={13} strokeWidth={1.8} style={{ color: cfg.color }} />
        <span className="m-group-title">{cfg.label}</span>
        <span className="m-group-count">{count}</span>
      </div>
      {issues.map((iss) => {
        const pri = PRIORITY[iss.priority]
        const Pri = pri.Icon
        const statusCfg = STATUS[iss.status]
        const StatusI = statusCfg.Icon
        return (
          <div
            key={iss.id}
            className={`m-row ${flashId === iss.id ? `is-flashing` : ``}`}
          >
            <Pri
              size={13}
              strokeWidth={2}
              style={{ color: pri.color }}
              className="m-row-pri"
            />
            <span className="m-row-ident">{iss.ident}</span>
            <StatusI
              size={13}
              strokeWidth={1.9}
              style={{ color: statusCfg.color }}
              className="m-row-status"
            />
            <span className="m-row-title">{iss.title}</span>
            <span className="m-row-trail">
              {iss.labels && iss.labels.length > 0 && (
                <span className="m-row-labels">
                  {iss.labels.slice(0, 3).map((l, i) => (
                    <span
                      key={i}
                      className="m-row-label-dot"
                      style={{ background: l.color }}
                    />
                  ))}
                </span>
              )}
              {iss.due && (
                <span className="m-row-due">
                  <Calendar size={9} strokeWidth={2} />
                  <span>{iss.due}</span>
                </span>
              )}
              <span className="m-row-assignee">{iss.assignee}</span>
            </span>
          </div>
        )
      })}
    </div>
  )
}
