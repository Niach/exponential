import { useEffect, useRef, useState } from "react"
import { AnimatePresence, motion } from "motion/react"
import {
  AlertTriangle,
  Bell,
  ChevronDown,
  ChevronRight,
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
import { MobileIssueDetail } from "./MobileIssueDetail"
import { MobileSearch } from "./MobileSearch"
import { MobileCreateSheet } from "./MobileCreateSheet"

type StatusKey = `backlog` | `todo` | `in_progress` | `done` | `cancelled`
type PriorityKey = `none` | `urgent` | `high` | `medium` | `low`
type TabKey = `all` | `active` | `backlog`

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

const STATUS: Record<StatusKey, { label: string; color: string; Icon: typeof CircleIcon }> = {
  backlog: { label: `Backlog`, color: `oklch(0.708 0 0)`, Icon: CircleDashed },
  todo: { label: `Todo`, color: `oklch(0.85 0 0)`, Icon: CircleIcon },
  in_progress: { label: `In Progress`, color: `oklch(0.795 0.184 86.05)`, Icon: Hourglass },
  done: { label: `Done`, color: `oklch(0.723 0.219 149.58)`, Icon: CircleCheck },
  cancelled: { label: `Cancelled`, color: `oklch(0.637 0.237 25.33)`, Icon: CircleX },
}

const PRIORITY: Record<PriorityKey, { color: string; Icon: typeof CircleIcon }> = {
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
  { id: `m1`, ident: `EXP-24`, title: `Email digest of stale issues`, status: `todo`, priority: `urgent`, due: `May 2`, labels: [{ name: `feature`, color: LABEL_FEATURE }], assignee: `D` },
  { id: `m2`, ident: `EXP-23`, title: `Bulk-edit selected issues`, status: `todo`, priority: `high`, labels: [{ name: `feature`, color: LABEL_FEATURE }], assignee: `N` },
  { id: `m3`, ident: `EXP-22`, title: `Drag to reorder within a status group`, status: `todo`, priority: `high`, labels: [{ name: `polish`, color: LABEL_POLISH }, { name: `ux`, color: LABEL_UX }], assignee: `D` },
  { id: `m4`, ident: `EXP-21`, title: `Issue templates per project`, status: `todo`, priority: `medium`, labels: [{ name: `feature`, color: LABEL_FEATURE }], assignee: `N` },
  { id: `m5`, ident: `EXP-19`, title: `GitHub PR linking via commit`, status: `todo`, priority: `low`, labels: [{ name: `integration`, color: LABEL_INTEGRATION }], assignee: `D` },
  { id: `m6`, ident: `EXP-18`, title: `Crash when opening empty workspace`, status: `in_progress`, priority: `urgent`, labels: [{ name: `bug`, color: LABEL_BUG }], assignee: `D` },
]

type Toast = { id: number; who: string; ident: string; text: string } | null

export function ProductMobile({ animate = true }: { animate?: boolean }) {
  const [issues, setIssues] = useState<Issue[]>(seed)
  const [flashId, setFlashId] = useState<string | null>(null)
  const [toast, setToast] = useState<Toast>(null)
  const [time, setTime] = useState(`9:41`)
  const [activeTab, setActiveTab] = useState<TabKey>(`all`)
  const [selectedIssue, setSelectedIssue] = useState<Issue | null>(null)
  const [userInteracted, setUserInteracted] = useState(false)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState(``)
  const [createOpen, setCreateOpen] = useState(false)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    setTime(
      new Date().toLocaleTimeString(`en-US`, {
        hour: `numeric`,
        minute: `2-digit`,
      })
    )
  }, [])

  useEffect(() => {
    if (!animate || userInteracted) return
    let tick = 0
    let toastId = 0
    const cycle = () => {
      tick++
      const m = tick % 4
      if (m === 1) {
        toastId++
        setToast({ id: toastId, who: `niach`, ident: `EXP-23`, text: `moved to In Progress` })
        setTimeout(() => {
          setIssues((xs) => xs.map((i) => (i.id === `m2` ? { ...i, status: `in_progress` } : i)))
          setFlashId(`m2`)
          setTimeout(() => setFlashId(null), 1000)
        }, 500)
        setTimeout(() => setToast(null), 2800)
      } else if (m === 2) {
        toastId++
        setToast({ id: toastId, who: `danny`, ident: `EXP-24`, text: `moved to In Progress` })
        setTimeout(() => {
          setIssues((xs) => xs.map((i) => (i.id === `m1` ? { ...i, status: `in_progress` } : i)))
          setFlashId(`m1`)
          setTimeout(() => setFlashId(null), 1000)
        }, 500)
        setTimeout(() => setToast(null), 2800)
      } else if (m === 3) {
        toastId++
        setToast({ id: toastId, who: `niach`, ident: `EXP-19`, text: `marked Done` })
        setTimeout(() => {
          setIssues((xs) => xs.map((i) => (i.id === `m5` ? { ...i, status: `done` } : i)))
          setFlashId(`m5`)
          setTimeout(() => setFlashId(null), 1000)
        }, 500)
        setTimeout(() => setToast(null), 2800)
      } else {
        setIssues(seed)
      }
    }
    cycle()
    intervalRef.current = setInterval(cycle, 3600)
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [animate, userInteracted])

  const handleTabSwitch = (tab: TabKey) => {
    setUserInteracted(true)
    setActiveTab(tab)
    setSelectedIssue(null)
  }

  const handleRowTap = (issue: Issue) => {
    setUserInteracted(true)
    setSelectedIssue(issue)
  }

  const handleSearch = () => {
    setUserInteracted(true)
    setSearchOpen(true)
  }

  const handleCreate = (title: string) => {
    setUserInteracted(true)
    const newId = `mnew-${Date.now()}`
    const newIssue: Issue = {
      id: newId,
      ident: `EXP-${issues.length + 25}`,
      title,
      status: `backlog`,
      priority: `none`,
      labels: [],
      assignee: `D`,
    }
    setIssues((xs) => [newIssue, ...xs])
    setFlashId(newId)
    setTimeout(() => setFlashId(null), 1100)
  }

  const filtered = issues.filter((i) => {
    if (activeTab === `active`) return i.status === `in_progress`
    if (activeTab === `backlog`) return i.status === `backlog` || i.status === `todo`
    return true
  }).filter((i) => {
    if (!searchQuery) return true
    return i.title.toLowerCase().includes(searchQuery.toLowerCase())
  })

  const inProgress = filtered.filter((i) => i.status === `in_progress`)
  const todo = filtered.filter((i) => i.status === `todo`)
  const done = filtered.filter((i) => i.status === `done`)
  const total = issues.length

  return (
    <motion.div
      className="phone"
      role="img"
      aria-label="Exponential iOS app"
      animate={{ y: [0, -5, 0] }}
      transition={{ repeat: Infinity, duration: 5, ease: `easeInOut` }}
    >
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

          <AnimatePresence mode="wait">
            {selectedIssue ? (
              <motion.div
                key="detail"
                className="m-app"
                initial={{ x: `100%` }}
                animate={{ x: 0 }}
                exit={{ x: `100%` }}
                transition={{ duration: 0.25, ease: [0.2, 0.8, 0.2, 1] }}
              >
                <MobileIssueDetail
                  issue={selectedIssue}
                  onBack={() => setSelectedIssue(null)}
                />
                <span className="m-home-indicator" aria-hidden />
              </motion.div>
            ) : (
              <motion.div
                key="list"
                className="m-app"
                initial={{ x: 0 }}
                animate={{ x: 0 }}
                exit={{ x: `-30%`, opacity: 0 }}
                transition={{ duration: 0.2 }}
              >
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
                      <button className="m-icon-btn" aria-label="Search" onClick={handleSearch}>
                        <Search size={14} strokeWidth={2} />
                      </button>
                      <button className="m-icon-btn" aria-label="New issue" onClick={() => { setUserInteracted(true); setCreateOpen(true) }}>
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
                  {([`all`, `active`, `backlog`] as TabKey[]).map((tab) => (
                    <button
                      key={tab}
                      className={`m-tab ${activeTab === tab ? `is-active` : ``}`}
                      onClick={() => handleTabSwitch(tab)}
                    >
                      {tab === `all` ? `All Issues` : tab === `active` ? `Active` : `Backlog`}
                    </button>
                  ))}
                </div>

                <AnimatePresence>
                  {searchOpen && (
                    <MobileSearch
                      query={searchQuery}
                      onChange={setSearchQuery}
                      onClose={() => { setSearchOpen(false); setSearchQuery(``) }}
                    />
                  )}
                </AnimatePresence>

                <div className="m-list">
                  {inProgress.length > 0 && (
                    <MGroup
                      kind="in_progress"
                      count={inProgress.length}
                      issues={inProgress}
                      flashId={flashId}
                      onTap={handleRowTap}
                    />
                  )}
                  {todo.length > 0 && (
                    <MGroup
                      kind="todo"
                      count={activeTab === `all` ? 18 : todo.length}
                      issues={todo}
                      flashId={flashId}
                      onTap={handleRowTap}
                    />
                  )}
                  {done.length > 0 && (
                    <MGroup
                      kind="done"
                      count={done.length}
                      issues={done}
                      flashId={flashId}
                      onTap={handleRowTap}
                    />
                  )}
                </div>

                <AnimatePresence>
                  {createOpen && (
                    <MobileCreateSheet
                      onClose={() => setCreateOpen(false)}
                      onCreate={handleCreate}
                    />
                  )}
                </AnimatePresence>

                <span className="m-home-indicator" aria-hidden />
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
    </motion.div>
  )
}

function MGroup({
  kind,
  count,
  issues,
  flashId,
  onTap,
}: {
  kind: StatusKey
  count: number
  issues: Issue[]
  flashId: string | null
  onTap: (issue: Issue) => void
}) {
  const cfg = STATUS[kind]
  const Sig = cfg.Icon
  return (
    <div className="m-group">
      <div className="m-group-head">
        <ChevronDown size={11} strokeWidth={2.2} style={{ color: `rgba(255,255,255,0.45)` }} />
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
            onClick={() => onTap(iss)}
          >
            <Pri size={13} strokeWidth={2} style={{ color: pri.color }} className="m-row-pri" />
            <span className="m-row-ident">{iss.ident}</span>
            <StatusI size={13} strokeWidth={1.9} style={{ color: statusCfg.color }} className="m-row-status" />
            <span className="m-row-title">{iss.title}</span>
            <span className="m-row-trail">
              {iss.labels && iss.labels.length > 0 && (
                <span className="m-row-labels">
                  {iss.labels.slice(0, 3).map((l, i) => (
                    <span key={i} className="m-row-label-dot" style={{ background: l.color }} />
                  ))}
                </span>
              )}
              <span className="m-row-assignee">{iss.assignee}</span>
            </span>
            <ChevronRight size={12} strokeWidth={2} className="m-row-chevron" />
          </div>
        )
      })}
    </div>
  )
}
