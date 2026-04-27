import { useEffect, useState } from "react"
import {
  AlertTriangle,
  Circle,
  CircleCheck,
  CircleDashed,
  CircleX,
  Minus,
  SignalHigh,
  SignalLow,
  SignalMedium,
  Timer,
  type LucideIcon,
} from "lucide-react"
import {
  IcCal,
  IcChevDown,
  IcChevSwap,
  IcFilter,
  IcPlus,
  IcSidebar,
  IcViewsEmpty,
} from "./icons"

type StatusKey = "backlog" | "todo" | "in_progress" | "done" | "cancelled"
type PriorityKey = "none" | "urgent" | "high" | "medium" | "low"

const STATUS: Record<
  StatusKey,
  { label: string; icon: LucideIcon; color: string }
> = {
  backlog: { label: "Backlog", icon: CircleDashed, color: "oklch(0.708 0 0)" },
  todo: { label: "Todo", icon: Circle, color: "oklch(0.985 0 0)" },
  in_progress: { label: "In Progress", icon: Timer, color: "oklch(0.795 0.184 86.05)" },
  done: { label: "Done", icon: CircleCheck, color: "oklch(0.723 0.219 149.58)" },
  cancelled: { label: "Cancelled", icon: CircleX, color: "oklch(0.708 0 0)" },
}

const PRIORITY: Record<
  PriorityKey,
  { label: string; icon: LucideIcon; color: string }
> = {
  none: { label: "No priority", icon: Minus, color: "oklch(0.708 0 0)" },
  urgent: { label: "Urgent", icon: AlertTriangle, color: "oklch(0.637 0.237 25.33)" },
  high: { label: "High", icon: SignalHigh, color: "oklch(0.705 0.213 47.6)" },
  medium: { label: "Medium", icon: SignalMedium, color: "oklch(0.795 0.184 86.05)" },
  low: { label: "Low", icon: SignalLow, color: "oklch(0.623 0.214 259.85)" },
}

function StatusIcon({ kind, size = 14 }: { kind: StatusKey; size?: number }) {
  const cfg = STATUS[kind]
  const Icon = cfg.icon
  return <Icon size={size} strokeWidth={1.7} style={{ color: cfg.color }} />
}

function PriorityIcon({ kind, size = 14 }: { kind: PriorityKey; size?: number }) {
  const cfg = PRIORITY[kind]
  const Icon = cfg.icon
  return <Icon size={size} strokeWidth={2} style={{ color: cfg.color }} />
}

type IssueLabel = { name: string; color: string }
type Issue = {
  id: string
  ident: string
  title: string
  status: StatusKey
  priority: PriorityKey
  labels?: IssueLabel[]
  due?: string
}

const seedIssues: Issue[] = [
  { id: "ex24", ident: "EXP-24", title: "Email digest of stale issues", status: "todo", priority: "urgent", labels: [{ name: "feature", color: "oklch(0.72 0.18 145)" }], due: "May 2" },
  { id: "ex23", ident: "EXP-23", title: "Bulk-edit selected issues", status: "todo", priority: "high", labels: [{ name: "feature", color: "oklch(0.72 0.18 145)" }] },
  { id: "ex22", ident: "EXP-22", title: "Drag to reorder within a status group", status: "todo", priority: "high", labels: [{ name: "polish", color: "oklch(0.72 0.16 280)" }, { name: "ux", color: "oklch(0.72 0.16 245)" }] },
  { id: "ex21", ident: "EXP-21", title: "Issue templates per project", status: "todo", priority: "medium", labels: [{ name: "feature", color: "oklch(0.72 0.18 145)" }] },
  { id: "ex20", ident: "EXP-20", title: "Markdown shortcuts in description editor", status: "todo", priority: "low", labels: [{ name: "editor", color: "oklch(0.72 0.16 280)" }, { name: "polish", color: "oklch(0.72 0.16 245)" }] },
  { id: "ex19", ident: "EXP-19", title: "GitHub PR linking via commit message", status: "todo", priority: "low", labels: [{ name: "integration", color: "oklch(0.7 0.04 245)" }, { name: "feature", color: "oklch(0.75 0.16 75)" }] },
  { id: "ex18", ident: "EXP-18", title: "Webhook events for issue mutations", status: "todo", priority: "low", labels: [{ name: "api", color: "oklch(0.7 0.15 320)" }, { name: "integration", color: "oklch(0.72 0.16 245)" }, { name: "feature", color: "oklch(0.75 0.16 75)" }] },
  { id: "ex17", ident: "EXP-17", title: "Slack notifications for assigned issues", status: "todo", priority: "low", labels: [{ name: "integration", color: "oklch(0.72 0.18 145)" }, { name: "feature", color: "oklch(0.72 0.16 245)" }] },
  { id: "ex16", ident: "EXP-16", title: "CSV export of filtered views", status: "todo", priority: "none", labels: [{ name: "feature", color: "oklch(0.7 0.15 320)" }, { name: "data", color: "oklch(0.72 0.16 245)" }] },
  { id: "ex15", ident: "EXP-15", title: "Webhook signing key rotation", status: "todo", priority: "low", labels: [{ name: "api", color: "oklch(0.7 0.15 320)" }, { name: "infra", color: "oklch(0.7 0.04 245)" }] },
  { id: "ex14", ident: "EXP-14", title: "Saved filter views in the sidebar", status: "todo", priority: "low", labels: [{ name: "feature", color: "oklch(0.75 0.16 75)" }] },
  { id: "ex13", ident: "EXP-13", title: "Mention users in issue descriptions", status: "todo", priority: "low", labels: [{ name: "editor", color: "oklch(0.72 0.18 145)" }, { name: "feature", color: "oklch(0.72 0.16 245)" }, { name: "ux", color: "oklch(0.75 0.16 75)" }] },
]

type Cursor = { visible: boolean; label: string; id: string | null }

export function ProductBoard({ animate = true }: { animate?: boolean }) {
  const [issues, setIssues] = useState<Issue[]>(seedIssues)
  const [flashId, setFlashId] = useState<string | null>(null)
  const [cursor, setCursor] = useState<Cursor>({
    visible: false,
    label: "danny",
    id: null,
  })

  useEffect(() => {
    if (!animate) {
      setIssues(seedIssues)
      return
    }
    let tick = 0
    const cycle = () => {
      tick++
      const m = tick % 4
      if (m === 1) {
        setCursor({ visible: true, label: "danny", id: "ex23" })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === "ex23" ? { ...i, status: "in_progress" } : i))
          )
          setFlashId("ex23")
          setTimeout(() => setFlashId(null), 1100)
        }, 700)
        setTimeout(() => setCursor((c) => ({ ...c, visible: false })), 1700)
      } else if (m === 2) {
        setCursor({ visible: true, label: "niach", id: "ex24" })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === "ex24" ? { ...i, status: "in_progress" } : i))
          )
          setFlashId("ex24")
          setTimeout(() => setFlashId(null), 1100)
        }, 700)
        setTimeout(() => setCursor((c) => ({ ...c, visible: false })), 1700)
      } else if (m === 3) {
        setCursor({ visible: true, label: "danny", id: "ex19" })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === "ex19" ? { ...i, status: "done" } : i))
          )
          setFlashId("ex19")
          setTimeout(() => setFlashId(null), 1100)
        }, 700)
        setTimeout(() => setCursor((c) => ({ ...c, visible: false })), 1700)
      } else {
        setIssues(seedIssues)
      }
    }
    cycle()
    const id = setInterval(cycle, 3800)
    return () => clearInterval(id)
  }, [animate])

  const todo = issues.filter((i) => i.status === "todo")
  const inProgress = issues.filter((i) => i.status === "in_progress")
  const done = issues.filter((i) => i.status === "done")

  return (
    <div className="ex-app">
      <aside className="ex-sidebar">
        <div className="ex-ws">
          <span className="ex-ws-avatar">A</span>
          <span className="ex-ws-name">Acme · Workspace</span>
          <IcChevSwap size={12} />
        </div>

        <div className="ex-side-section">
          <div className="ex-side-label">
            <span>Projects</span>
            <button className="ex-side-add" aria-label="Add project">
              <IcPlus size={12} />
            </button>
          </div>
          <div className="ex-side-item is-active">
            <span
              className="ex-proj-dot"
              style={{ background: "oklch(0.62 0.18 280)" }}
            />
            <span>Exponential</span>
          </div>
          <div className="ex-side-item">
            <span
              className="ex-proj-dot"
              style={{ background: "oklch(0.7 0.16 145)" }}
            />
            <span>Marketing site</span>
          </div>
          <div className="ex-side-item">
            <span
              className="ex-proj-dot"
              style={{ background: "oklch(0.72 0.16 60)" }}
            />
            <span>Mobile app</span>
          </div>
        </div>

        <div className="ex-side-section">
          <div className="ex-side-label">
            <span>Views</span>
          </div>
          <div className="ex-side-item ex-side-empty">
            <IcViewsEmpty size={13} />
            <span>No views yet</span>
          </div>
        </div>

        <div className="ex-side-user">
          <span className="ex-user-avatar">D</span>
          <span className="ex-user-mail">danny@acme.io</span>
          <IcChevSwap size={11} />
        </div>
      </aside>

      <div className="ex-main">
        <div className="ex-titlebar">
          <button className="ex-icon-btn" aria-label="Toggle sidebar">
            <IcSidebar size={15} />
          </button>
        </div>

        <div className="ex-header">
          <h1 className="ex-h1">Issues</h1>
          <div className="ex-header-right">
            <button className="ex-filter-btn">
              <IcFilter size={13} /> Filter
            </button>
            <button className="ex-new-btn">
              <IcPlus size={13} /> New Issue
            </button>
          </div>
        </div>

        <div className="ex-tabs">
          <div className="ex-tab is-active">All Issues</div>
          <div className="ex-tab">Active</div>
          <div className="ex-tab">Backlog</div>
        </div>

        {inProgress.length > 0 && (
          <Group
            title="In Progress"
            kind="in_progress"
            count={inProgress.length}
            issues={inProgress}
            flashId={flashId}
            cursor={cursor}
          />
        )}
        {todo.length > 0 && (
          <Group
            title="Todo"
            kind="todo"
            count={todo.length === 12 ? 18 : todo.length}
            issues={todo}
            flashId={flashId}
            cursor={cursor}
          />
        )}
        {done.length > 0 && (
          <Group
            title="Done"
            kind="done"
            count={done.length}
            issues={done}
            flashId={flashId}
            cursor={cursor}
          />
        )}
      </div>
    </div>
  )
}

function Group({
  title,
  kind,
  count,
  issues,
  flashId,
  cursor,
}: {
  title: string
  kind: StatusKey
  count: number
  issues: Issue[]
  flashId: string | null
  cursor: Cursor
}) {
  return (
    <>
      <div className="ex-group">
        <IcChevDown size={12} style={{ color: "var(--ex-fg-dim)" }} />
        <StatusIcon kind={kind} />
        <span className="ex-group-title">{title}</span>
        <span className="ex-group-count">{count}</span>
      </div>
      {issues.map((iss) => (
        <div
          key={iss.id}
          className={`ex-row ${flashId === iss.id ? "is-flashing" : ""}`}
        >
          <span className="ex-pri">
            <PriorityIcon kind={iss.priority} size={13} />
          </span>
          <span className="ex-ident">{iss.ident}</span>
          <StatusIcon kind={iss.status} />
          <span className="ex-title">{iss.title}</span>
          <span className="ex-labels">
            {iss.labels?.map((l, i) => (
              <span key={i} className="ex-label">
                <span
                  className="ex-label-dot"
                  style={{ background: l.color }}
                />
                {l.name}
              </span>
            ))}
          </span>
          <span className="ex-assignee" title="Danny">
            D
          </span>
          <span className={`ex-due ${iss.due ? "" : "is-empty"}`}>
            <IcCal size={12} />
            {iss.due && <span>{iss.due}</span>}
          </span>
          {cursor.visible && cursor.id === iss.id && (
            <span className="ex-cursor">
              <span className="ex-cursor-dot" />
              {cursor.label}
            </span>
          )}
        </div>
      ))}
    </>
  )
}

