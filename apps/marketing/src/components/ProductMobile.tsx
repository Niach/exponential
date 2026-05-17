import { useEffect, useState } from "react"
import {
  AlertTriangle,
  Bell,
  CircleCheck,
  CircleDashed,
  Circle as CircleIcon,
  House,
  Inbox,
  Plus,
  SignalHigh,
  SignalLow,
  SignalMedium,
  Timer,
  User as UserIcon,
} from "lucide-react"

type StatusKey = `backlog` | `todo` | `in_progress` | `done`
type PriorityKey = `urgent` | `high` | `medium` | `low`

type Issue = {
  id: string
  ident: string
  title: string
  status: StatusKey
  priority: PriorityKey
  due?: string
  meta?: string
}

const STATUS: Record<
  StatusKey,
  { label: string; color: string; Icon: typeof CircleIcon }
> = {
  backlog: { label: `Backlog`, color: `oklch(0.7 0 0)`, Icon: CircleDashed },
  todo: { label: `Todo`, color: `oklch(0.985 0 0)`, Icon: CircleIcon },
  in_progress: {
    label: `In Progress`,
    color: `oklch(0.795 0.184 86.05)`,
    Icon: Timer,
  },
  done: {
    label: `Done`,
    color: `oklch(0.723 0.219 149.58)`,
    Icon: CircleCheck,
  },
}

const PRIORITY: Record<
  PriorityKey,
  { color: string; Icon: typeof CircleIcon }
> = {
  urgent: { color: `oklch(0.637 0.237 25.33)`, Icon: AlertTriangle },
  high: { color: `oklch(0.705 0.213 47.6)`, Icon: SignalHigh },
  medium: { color: `oklch(0.795 0.184 86.05)`, Icon: SignalMedium },
  low: { color: `oklch(0.623 0.214 259.85)`, Icon: SignalLow },
}

const seed: Issue[] = [
  {
    id: `m1`,
    ident: `EXP-24`,
    title: `Email digest of stale issues`,
    status: `todo`,
    priority: `urgent`,
    due: `May 2`,
    meta: `feature`,
  },
  {
    id: `m2`,
    ident: `EXP-23`,
    title: `Bulk-edit selected issues`,
    status: `todo`,
    priority: `high`,
    meta: `feature`,
  },
  {
    id: `m3`,
    ident: `EXP-22`,
    title: `Drag to reorder within a status group`,
    status: `todo`,
    priority: `high`,
    meta: `polish · ux`,
  },
  {
    id: `m4`,
    ident: `EXP-21`,
    title: `Issue templates per project`,
    status: `todo`,
    priority: `medium`,
    meta: `feature`,
  },
  {
    id: `m5`,
    ident: `EXP-19`,
    title: `GitHub PR linking via commit`,
    status: `todo`,
    priority: `low`,
    meta: `integration`,
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
          who: `danny`,
          ident: `EXP-23`,
          text: `moved to In Progress`,
        })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `m2` ? { ...i, status: `in_progress` } : i))
          )
          setFlashId(`m2`)
          setTimeout(() => setFlashId(null), 900)
        }, 500)
        setTimeout(() => setToast(null), 2800)
      } else if (m === 2) {
        toastId++
        setToast({
          id: toastId,
          who: `niach`,
          ident: `EXP-24`,
          text: `moved to In Progress`,
        })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `m1` ? { ...i, status: `in_progress` } : i))
          )
          setFlashId(`m1`)
          setTimeout(() => setFlashId(null), 900)
        }, 500)
        setTimeout(() => setToast(null), 2800)
      } else if (m === 3) {
        toastId++
        setToast({
          id: toastId,
          who: `danny`,
          ident: `EXP-19`,
          text: `marked Done`,
        })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `m5` ? { ...i, status: `done` } : i))
          )
          setFlashId(`m5`)
          setTimeout(() => setFlashId(null), 900)
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

  return (
    <div className="phone" role="img" aria-label="Exponential mobile app">
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

            <div className="m-header">
              <div>
                <div className="m-workspace">
                  <span className="m-ws-avatar">A</span>
                  <span>Acme</span>
                </div>
                <h2 className="m-title">Issues</h2>
              </div>
              <button className="m-new" aria-label="New issue">
                <Plus size={16} strokeWidth={2.2} />
              </button>
            </div>

            <div className="m-tabs">
              <button className="m-tab is-active">All</button>
              <button className="m-tab">Active</button>
              <button className="m-tab">Backlog</button>
            </div>

            <div className="m-list">
              {inProgress.length > 0 && (
                <MGroup
                  title="In Progress"
                  kind="in_progress"
                  count={inProgress.length}
                  issues={inProgress}
                  flashId={flashId}
                />
              )}
              {todo.length > 0 && (
                <MGroup
                  title="Todo"
                  kind="todo"
                  count={todo.length === 5 ? 18 : 18 - (5 - todo.length)}
                  issues={todo}
                  flashId={flashId}
                />
              )}
              {done.length > 0 && (
                <MGroup
                  title="Done"
                  kind="done"
                  count={done.length}
                  issues={done}
                  flashId={flashId}
                />
              )}
            </div>

            <div className="m-bottom">
              <div className="m-bottom-item is-active">
                <Inbox size={18} strokeWidth={1.8} />
                <span>Issues</span>
              </div>
              <div className="m-bottom-item">
                <House size={18} strokeWidth={1.8} />
                <span>Projects</span>
              </div>
              <div className="m-bottom-item">
                <Bell size={18} strokeWidth={1.8} />
                <span>Inbox</span>
              </div>
              <div className="m-bottom-item">
                <UserIcon size={18} strokeWidth={1.8} />
                <span>You</span>
              </div>
              <span className="m-home-indicator" aria-hidden />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function MGroup({
  title,
  kind,
  count,
  issues,
  flashId,
}: {
  title: string
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
        <Sig size={13} strokeWidth={1.8} style={{ color: cfg.color }} />
        <span className="m-group-title">{title}</span>
        <span className="m-group-count">{count}</span>
      </div>
      {issues.map((iss) => {
        const pri = PRIORITY[iss.priority]
        const Pri = pri.Icon
        return (
          <div
            key={iss.id}
            className={`m-row ${flashId === iss.id ? `is-flashing` : ``}`}
          >
            <span className="m-row-icon" style={{ color: cfg.color }}>
              <Sig size={15} strokeWidth={1.8} />
            </span>
            <div className="m-row-body">
              <div className="m-row-top">
                <span className="m-ident">{iss.ident}</span>
                <Pri
                  size={12}
                  strokeWidth={2}
                  style={{ color: pri.color }}
                />
                {iss.due && <span className="m-due">{iss.due}</span>}
              </div>
              <div className="m-row-title">{iss.title}</div>
              {iss.meta && <div className="m-row-meta">{iss.meta}</div>}
            </div>
          </div>
        )
      })}
    </div>
  )
}
