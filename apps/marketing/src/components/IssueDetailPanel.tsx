import { useEffect, useState } from "react"
import { motion } from "motion/react"
import {
  AlertTriangle,
  CalendarDays,
  Check,
  Circle,
  CircleCheck,
  CircleDashed,
  CircleX,
  Minus,
  SignalHigh,
  SignalLow,
  SignalMedium,
  Sparkles,
  Timer,
  X,
  type LucideIcon,
} from "lucide-react"

type StatusKey = `backlog` | `todo` | `in_progress` | `done` | `cancelled`
type PriorityKey = `none` | `urgent` | `high` | `medium` | `low`

const STATUS: Record<StatusKey, { label: string; icon: LucideIcon; color: string }> = {
  backlog: { label: `Backlog`, icon: CircleDashed, color: `oklch(0.708 0 0)` },
  todo: { label: `Todo`, icon: Circle, color: `oklch(0.985 0 0)` },
  in_progress: { label: `In Progress`, icon: Timer, color: `oklch(0.795 0.184 86.05)` },
  done: { label: `Done`, icon: CircleCheck, color: `oklch(0.723 0.219 149.58)` },
  cancelled: { label: `Cancelled`, icon: CircleX, color: `oklch(0.708 0 0)` },
}

const PRIORITY: Record<PriorityKey, { label: string; icon: LucideIcon; color: string }> = {
  none: { label: `No priority`, icon: Minus, color: `oklch(0.708 0 0)` },
  urgent: { label: `Urgent`, icon: AlertTriangle, color: `oklch(0.637 0.237 25.33)` },
  high: { label: `High`, icon: SignalHigh, color: `oklch(0.705 0.213 47.6)` },
  medium: { label: `Medium`, icon: SignalMedium, color: `oklch(0.795 0.184 86.05)` },
  low: { label: `Low`, icon: SignalLow, color: `oklch(0.623 0.214 259.85)` },
}

export function IssueDetailPanel({
  issue,
  projectName,
  projectColor,
  onClose,
}: {
  issue: {
    id: string
    ident: string
    title: string
    status: StatusKey
    priority: PriorityKey
    labels?: { name: string; color: string }[]
    due?: string
  }
  projectName: string
  projectColor: string
  onClose: () => void
}) {
  const [approved, setApproved] = useState(false)

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === `Escape`) onClose()
    }
    window.addEventListener(`keydown`, handler)
    return () => window.removeEventListener(`keydown`, handler)
  }, [onClose])

  const handleApprove = () => {
    setApproved(true)
    setTimeout(() => setApproved(false), 1500)
  }

  const statusCfg = STATUS[issue.status]
  const StatusIcon = statusCfg.icon
  const prioCfg = PRIORITY[issue.priority]
  const PrioIcon = prioCfg.icon

  return (
    <>
      <motion.div
        className="ex-detail-overlay"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.15 }}
        onClick={onClose}
      />
      <motion.div
        className="ex-detail-panel"
        initial={{ x: `100%` }}
        animate={{ x: 0 }}
        exit={{ x: `100%` }}
        transition={{ type: `spring`, damping: 30, stiffness: 300 }}
      >
        <div className="ex-detail-header">
          <span
            className="ex-detail-breadcrumb-dot"
            style={{ background: projectColor }}
          />
          <span>{projectName}</span>
          <span style={{ color: `var(--fg-dim)` }}>&rsaquo;</span>
          <span className="ex-detail-breadcrumb-ident">{issue.ident}</span>
          <button className="ex-detail-close" onClick={onClose}>
            <X size={16} strokeWidth={1.6} />
          </button>
        </div>

        <div className="ex-detail-content">
          <div className="ex-detail-main">
            <h2 className="ex-detail-title">{issue.title}</h2>
            <p className="ex-detail-desc">
              Implement the core logic for this feature. Need to coordinate with
              the team on the API design and handle edge cases.
            </p>

            <div className="ex-detail-divider" />

            <div className="ex-detail-timeline-label">Activity</div>

            <div className="ex-detail-activity">
              <span>Danny changed status to In Progress &middot; 3h ago</span>
            </div>

            <div className="ex-detail-comment">
              <span className="ex-detail-comment-avatar">D</span>
              <div className="ex-detail-comment-body">
                <div className="ex-detail-comment-meta">
                  <strong>Danny</strong>
                  <span>&middot; 2h ago</span>
                </div>
                <div className="ex-detail-comment-text">
                  Let's prioritize this for the next sprint. The API design
                  needs a review first.
                </div>
              </div>
            </div>

            <div className="ex-detail-comment">
              <span className="ex-detail-comment-avatar is-agent">
                <Sparkles size={13} strokeWidth={1.8} />
              </span>
              <div className="ex-detail-comment-body">
                <div className="ex-detail-comment-meta">
                  <strong>Claude</strong>
                  <span className="ex-detail-plan-tag">
                    Plan &middot; rev 1
                  </span>
                  <span>&middot; just now</span>
                </div>
                <div className="ex-detail-plan-card">
                  <div className="ex-detail-comment-text">
                    {`1. Add the core module with input validation\n2. Wire up the API endpoint\n3. Add integration tests for edge cases`}
                  </div>
                  <div className="ex-detail-plan-actions">
                    {approved ? (
                      <button className="ex-detail-plan-btn is-approved">
                        <Check size={12} strokeWidth={2} /> Approved
                      </button>
                    ) : (
                      <button
                        className="ex-detail-plan-btn is-primary"
                        onClick={handleApprove}
                      >
                        Approve
                      </button>
                    )}
                    <button className="ex-detail-plan-btn">
                      Request changes
                    </button>
                  </div>
                </div>
              </div>
            </div>

            <div className="ex-detail-comment-input">
              Add a comment...
            </div>
          </div>

          <div className="ex-detail-sidebar">
            <div className="ex-detail-prop">
              <div className="ex-detail-prop-label">Status</div>
              <div className="ex-detail-prop-value">
                <StatusIcon
                  size={14}
                  strokeWidth={1.7}
                  style={{ color: statusCfg.color }}
                />
                {statusCfg.label}
              </div>
            </div>

            <div className="ex-detail-prop">
              <div className="ex-detail-prop-label">Priority</div>
              <div className="ex-detail-prop-value">
                <PrioIcon
                  size={14}
                  strokeWidth={2}
                  style={{ color: prioCfg.color }}
                />
                {prioCfg.label}
              </div>
            </div>

            <div className="ex-detail-prop">
              <div className="ex-detail-prop-label">Assignee</div>
              <div className="ex-detail-prop-value">
                <span className="ex-detail-prop-avatar">D</span>
                Danny
              </div>
            </div>

            {issue.labels && issue.labels.length > 0 && (
              <div className="ex-detail-prop">
                <div className="ex-detail-prop-label">Labels</div>
                <div style={{ display: `flex`, flexDirection: `column`, gap: 4 }}>
                  {issue.labels.map((l, i) => (
                    <span key={i} className="ex-detail-prop-label-pill">
                      <span
                        className="ex-detail-prop-label-dot"
                        style={{ background: l.color }}
                      />
                      {l.name}
                    </span>
                  ))}
                </div>
              </div>
            )}

            {issue.due && (
              <div className="ex-detail-prop">
                <div className="ex-detail-prop-label">Due date</div>
                <div className="ex-detail-prop-value">
                  <CalendarDays size={13} strokeWidth={1.6} />
                  {issue.due}
                </div>
              </div>
            )}
          </div>
        </div>
      </motion.div>
    </>
  )
}
