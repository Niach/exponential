import { useEffect, useRef, useState } from "react"
import { AnimatePresence, motion } from "motion/react"
import {
  AlertTriangle,
  Circle,
  CircleCheck,
  CircleDashed,
  CircleUserRound,
  CircleX,
  Inbox,
  Megaphone,
  Minus,
  Search,
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
} from "./icons"
import { CreateIssueDialog } from "./CreateIssueDialog"
import { IssueDetailPanel } from "./IssueDetailPanel"

type StatusKey = `backlog` | `todo` | `in_progress` | `done` | `cancelled`
type PriorityKey = `none` | `urgent` | `high` | `medium` | `low`
type TabKey = `all` | `active` | `backlog`

const STATUS: Record<
  StatusKey,
  { label: string; icon: LucideIcon; color: string }
> = {
  backlog: { label: `Backlog`, icon: CircleDashed, color: `oklch(0.708 0 0)` },
  todo: { label: `Todo`, icon: Circle, color: `oklch(0.985 0 0)` },
  in_progress: { label: `In Progress`, icon: Timer, color: `oklch(0.795 0.184 86.05)` },
  done: { label: `Done`, icon: CircleCheck, color: `oklch(0.723 0.219 149.58)` },
  cancelled: { label: `Cancelled`, icon: CircleX, color: `oklch(0.708 0 0)` },
}

const PRIORITY: Record<
  PriorityKey,
  { label: string; icon: LucideIcon; color: string }
> = {
  none: { label: `No priority`, icon: Minus, color: `oklch(0.5 0 0)` },
  urgent: { label: `Urgent`, icon: AlertTriangle, color: `oklch(0.637 0.237 25.33)` },
  high: { label: `High`, icon: SignalHigh, color: `oklch(0.78 0 0)` },
  medium: { label: `Medium`, icon: SignalMedium, color: `oklch(0.7 0 0)` },
  low: { label: `Low`, icon: SignalLow, color: `oklch(0.58 0 0)` },
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
  assignee?: string
}

const BUG: IssueLabel = { name: `Bug`, color: `#ef4444` }
const FEATURE: IssueLabel = { name: `Feature`, color: `#8b5cf6` }
const DESIGN: IssueLabel = { name: `Design`, color: `#f59e0b` }

const PROJECTS: Record<string, { name: string; color: string; prefix: string; issues: Issue[] }> = {
  exponential: {
    name: `Exponential`,
    color: `oklch(0.62 0.18 280)`,
    prefix: `EXP`,
    issues: [
      { id: `exp2`, ident: `EXP-2`, title: `Fix duplicate push notifications on mention`, status: `in_progress`, priority: `urgent`, labels: [BUG], due: `Jun 11`, assignee: `D` },
      { id: `exp1`, ident: `EXP-1`, title: `Stream agent output to the plan panel`, status: `in_progress`, priority: `high`, labels: [FEATURE], assignee: `D` },
      { id: `exp3`, ident: `EXP-3`, title: `Attachment previews lose aspect ratio on Android`, status: `todo`, priority: `medium`, labels: [BUG], due: `Jun 16` },
      { id: `exp5`, ident: `EXP-5`, title: `Polish empty states for the notifications inbox`, status: `todo`, priority: `medium`, labels: [DESIGN], assignee: `D` },
      { id: `exp4`, ident: `EXP-4`, title: `Add keyboard shortcut for cycling issue status`, status: `todo`, priority: `low`, labels: [FEATURE] },
      { id: `exp9`, ident: `EXP-9`, title: `Webhook events for issue mutations`, status: `todo`, priority: `low`, labels: [FEATURE] },
      { id: `exp7`, ident: `EXP-7`, title: `Refresh workspace invite email template`, status: `backlog`, priority: `low`, labels: [DESIGN] },
      { id: `exp6`, ident: `EXP-6`, title: `Recurring issues: support monthly on last weekday`, status: `backlog`, priority: `none` },
      { id: `exp8`, ident: `EXP-8`, title: `Rate-limit workspace invite creation`, status: `done`, priority: `high`, assignee: `D`, due: `Jun 8` },
    ],
  },
  mobile: {
    name: `Mobile app`,
    color: `oklch(0.72 0.16 60)`,
    prefix: `MOB`,
    issues: [
      { id: `mo6`, ident: `MOB-6`, title: `Push notification deep links`, status: `in_progress`, priority: `high`, labels: [FEATURE], assignee: `D` },
      { id: `mo5`, ident: `MOB-5`, title: `Offline queue for issue mutations`, status: `todo`, priority: `urgent`, labels: [BUG], due: `Jun 13` },
      { id: `mo4`, ident: `MOB-4`, title: `Multi-server account switching`, status: `todo`, priority: `high`, labels: [FEATURE] },
      { id: `mo3`, ident: `MOB-3`, title: `Label color picker in create dialog`, status: `todo`, priority: `medium`, labels: [DESIGN] },
      { id: `mo2`, ident: `MOB-2`, title: `Attachment previews in issue detail`, status: `done`, priority: `medium`, labels: [FEATURE], assignee: `D` },
      { id: `mo1`, ident: `MOB-1`, title: `Android Compose navigation transitions`, status: `done`, priority: `low`, labels: [DESIGN] },
    ],
  },
}

const PROJECT_KEYS = Object.keys(PROJECTS) as (keyof typeof PROJECTS)[]

export function ProductBoard({ animate = true }: { animate?: boolean }) {
  const [projectKey, setProjectKey] = useState<string>(`exponential`)
  const [issues, setIssues] = useState<Issue[]>(PROJECTS.exponential.issues)
  const [flashId, setFlashId] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<TabKey>(`all`)
  const [cursor, setCursor] = useState<{ visible: boolean; label: string; id: string | null }>({
    visible: false,
    label: `danny`,
    id: null,
  })
  const [statusDropdown, setStatusDropdown] = useState<string | null>(null)
  const [userInteracted, setUserInteracted] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [selectedIssue, setSelectedIssue] = useState<Issue | null>(null)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const switchProject = (key: string) => {
    setUserInteracted(true)
    setProjectKey(key)
    setIssues(PROJECTS[key].issues)
    setActiveTab(`all`)
    setStatusDropdown(null)
  }

  const switchTab = (tab: TabKey) => {
    setUserInteracted(true)
    setActiveTab(tab)
    setStatusDropdown(null)
  }

  const changeStatus = (issueId: string, newStatus: StatusKey) => {
    setUserInteracted(true)
    setIssues((xs) => xs.map((i) => (i.id === issueId ? { ...i, status: newStatus } : i)))
    setFlashId(issueId)
    setTimeout(() => setFlashId(null), 1100)
    setStatusDropdown(null)
  }

  const handleCreate = (title: string) => {
    const proj = PROJECTS[projectKey]
    const maxNum = issues.reduce((max, iss) => {
      const m = iss.ident.match(/-(\d+)$/)
      return m ? Math.max(max, parseInt(m[1], 10)) : max
    }, 0)
    const nextNum = maxNum + 1
    const newId = `new-${Date.now()}`
    const newIssue: Issue = {
      id: newId,
      ident: `${proj.prefix}-${nextNum}`,
      title,
      status: `backlog`,
      priority: `none`,
    }
    setIssues((xs) => [newIssue, ...xs])
    setFlashId(newId)
    setTimeout(() => setFlashId(null), 1100)
  }

  useEffect(() => {
    if (!animate || userInteracted) return
    let tick = 0
    const cycle = () => {
      tick++
      const m = tick % 4
      if (m === 1) {
        setCursor({ visible: true, label: `danny`, id: `exp3` })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `exp3` ? { ...i, status: `in_progress` } : i))
          )
          setFlashId(`exp3`)
          setTimeout(() => setFlashId(null), 1100)
        }, 700)
        setTimeout(() => setCursor((c) => ({ ...c, visible: false })), 1700)
      } else if (m === 2) {
        setCursor({ visible: true, label: `alex`, id: `exp9` })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `exp9` ? { ...i, status: `in_progress` } : i))
          )
          setFlashId(`exp9`)
          setTimeout(() => setFlashId(null), 1100)
        }, 700)
        setTimeout(() => setCursor((c) => ({ ...c, visible: false })), 1700)
      } else if (m === 3) {
        setCursor({ visible: true, label: `danny`, id: `exp1` })
        setTimeout(() => {
          setIssues((xs) =>
            xs.map((i) => (i.id === `exp1` ? { ...i, status: `done` } : i))
          )
          setFlashId(`exp1`)
          setTimeout(() => setFlashId(null), 1100)
        }, 700)
        setTimeout(() => setCursor((c) => ({ ...c, visible: false })), 1700)
      } else {
        setIssues(PROJECTS.exponential.issues)
      }
    }
    cycle()
    intervalRef.current = setInterval(cycle, 3800)
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [animate, userInteracted])

  const filtered = issues.filter((i) => {
    if (activeTab === `active`) return i.status === `in_progress`
    if (activeTab === `backlog`) return i.status === `backlog` || i.status === `todo`
    return true
  })

  const groups: { key: StatusKey; title: string; items: Issue[] }[] = []
  for (const key of [`in_progress`, `todo`, `backlog`, `done`, `cancelled`] as StatusKey[]) {
    const items = filtered.filter((i) => i.status === key)
    if (items.length > 0) groups.push({ key, title: STATUS[key].label, items })
  }

  return (
    <>
    <div className="ex-app" onClick={() => setStatusDropdown(null)}>
      <aside className="ex-sidebar">
        <div className="ex-ws">
          <span className="ex-ws-avatar">A</span>
          <span className="ex-ws-name">Acme</span>
          <IcChevSwap size={12} />
        </div>

        <div className="ex-nav">
          <div className="ex-nav-item">
            <Search size={15} strokeWidth={1.8} />
            <span>Search</span>
          </div>
          <div className="ex-nav-item">
            <Inbox size={15} strokeWidth={1.8} />
            <span>Inbox</span>
          </div>
        </div>

        <div className="ex-side-section">
          <div className="ex-side-label">
            <span>Projects</span>
            <button className="ex-side-add" aria-label="Add project">
              <IcPlus size={12} />
            </button>
          </div>
          {PROJECT_KEYS.map((k) => {
            const p = PROJECTS[k]
            return (
              <div
                key={k}
                className={`ex-side-item ${k === projectKey ? `is-active` : ``}`}
                onClick={() => switchProject(k)}
              >
                <span className="ex-proj-dot" style={{ background: p.color }} />
                <span>{p.name}</span>
              </div>
            )
          })}
        </div>

        <div className="ex-side-foot">
          <div className="ex-nav-item">
            <Megaphone size={14} strokeWidth={1.8} />
            <span>Send feedback</span>
          </div>
          <div className="ex-side-user">
            <span className="ex-user-avatar">D</span>
            <span className="ex-user-mail">danny@acme.io</span>
            <IcChevSwap size={11} />
          </div>
        </div>
      </aside>

      <AnimatePresence mode="wait">
        {selectedIssue ? (
          <motion.div
            key="detail"
            className="ex-main"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
          >
            <IssueDetailPanel
              issue={selectedIssue}
              projectName={PROJECTS[projectKey].name}
              projectColor={PROJECTS[projectKey].color}
              onClose={() => setSelectedIssue(null)}
            />
          </motion.div>
        ) : (
          <motion.div
            key="list"
            className="ex-main"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
          >
            <div className="ex-header">
              <h1 className="ex-h1">Issues</h1>
              <div className="ex-header-right">
                <button className="ex-filter-btn">
                  <IcFilter size={13} /> Filter
                </button>
                <button className="ex-new-btn" onClick={() => { setUserInteracted(true); setCreateOpen(true) }}>
                  <IcPlus size={13} /> New Issue
                </button>
              </div>
            </div>

            <div className="ex-tabs">
              {([`all`, `active`, `backlog`] as TabKey[]).map((tab) => (
                <div
                  key={tab}
                  className={`ex-tab ${activeTab === tab ? `is-active` : ``}`}
                  onClick={() => switchTab(tab)}
                >
                  {tab === `all` ? `All Issues` : tab === `active` ? `Active` : `Backlog`}
                </div>
              ))}
            </div>

            {groups.map((g) => (
              <Group
                key={g.key}
                title={g.title}
                kind={g.key}
                count={g.items.length}
                issues={g.items}
                flashId={flashId}
                cursor={cursor}
                statusDropdown={statusDropdown}
                onStatusClick={(id, e) => {
                  e.stopPropagation()
                  setStatusDropdown(statusDropdown === id ? null : id)
                }}
                onStatusChange={changeStatus}
                onRowClick={(iss) => { setUserInteracted(true); setSelectedIssue(iss) }}
              />
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
    <AnimatePresence>
      {createOpen && (
        <CreateIssueDialog
          open={createOpen}
          onClose={() => setCreateOpen(false)}
          onCreate={handleCreate}
          projectPrefix={PROJECTS[projectKey].prefix}
          projectColor={PROJECTS[projectKey].color}
        />
      )}
    </AnimatePresence>
    </>
  )
}

function Group({
  title,
  kind,
  count,
  issues,
  flashId,
  cursor,
  statusDropdown,
  onStatusClick,
  onStatusChange,
  onRowClick,
}: {
  title: string
  kind: StatusKey
  count: number
  issues: Issue[]
  flashId: string | null
  cursor: { visible: boolean; label: string; id: string | null }
  statusDropdown: string | null
  onStatusClick: (id: string, e: React.MouseEvent) => void
  onStatusChange: (id: string, status: StatusKey) => void
  onRowClick: (issue: Issue) => void
}) {
  return (
    <>
      <div className={`ex-group is-${kind}`}>
        <IcChevDown size={12} style={{ color: `var(--ex-fg-dim)` }} />
        <StatusIcon kind={kind} />
        <span className="ex-group-title">{title}</span>
        <span className="ex-group-count">{count}</span>
      </div>
      {issues.map((iss) => (
        <div
          key={iss.id}
          className={`ex-row ${flashId === iss.id ? `is-flashing` : ``}`}
          onClick={() => onRowClick(iss)}
        >
          <span className="ex-pri">
            <PriorityIcon kind={iss.priority} size={13} />
          </span>
          <span className="ex-ident">{iss.ident}</span>
          <span
            className="ex-status-click"
            onClick={(e) => onStatusClick(iss.id, e)}
          >
            <StatusIcon kind={iss.status} />
            {statusDropdown === iss.id && (
              <StatusDropdown
                current={iss.status}
                onChange={(s) => onStatusChange(iss.id, s)}
              />
            )}
          </span>
          <span className="ex-title">{iss.title}</span>
          <span className="ex-labels">
            {iss.labels?.map((l, i) => (
              <span key={i} className="ex-label">
                <span className="ex-label-dot" style={{ background: l.color }} />
                {l.name}
              </span>
            ))}
          </span>
          {iss.assignee ? (
            <span className="ex-assignee" title="Danny">{iss.assignee}</span>
          ) : (
            <span className="ex-assignee is-empty">
              <CircleUserRound size={16} strokeWidth={1.5} />
            </span>
          )}
          <span className={`ex-due ${iss.due ? `` : `is-empty`}`}>
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

function StatusDropdown({
  current,
  onChange,
}: {
  current: StatusKey
  onChange: (s: StatusKey) => void
}) {
  return (
    <div className="ex-status-dropdown" onClick={(e) => e.stopPropagation()}>
      {(Object.keys(STATUS) as StatusKey[]).map((key) => (
        <div
          key={key}
          className={`ex-status-option ${key === current ? `is-current` : ``}`}
          onClick={() => onChange(key)}
        >
          <StatusIcon kind={key} size={13} />
          <span>{STATUS[key].label}</span>
        </div>
      ))}
    </div>
  )
}
