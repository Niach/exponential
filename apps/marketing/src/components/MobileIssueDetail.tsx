import {
  AlertTriangle,
  Calendar,
  ChevronLeft,
  CircleCheck,
  CircleDashed,
  Circle as CircleIcon,
  CircleX,
  Hourglass,
  MessageSquare,
  Radio,
  Sparkles,
  Minus,
  SignalHigh,
  SignalLow,
  SignalMedium,
} from "lucide-react"

type StatusKey = `backlog` | `todo` | `in_progress` | `done` | `cancelled`
type PriorityKey = `none` | `urgent` | `high` | `medium` | `low`

const STATUS: Record<StatusKey, { label: string; color: string; Icon: typeof CircleIcon }> = {
  backlog: { label: `Backlog`, color: `oklch(0.708 0 0)`, Icon: CircleDashed },
  todo: { label: `Todo`, color: `oklch(0.85 0 0)`, Icon: CircleIcon },
  in_progress: { label: `In Progress`, color: `oklch(0.795 0.184 86.05)`, Icon: Hourglass },
  done: { label: `Done`, color: `oklch(0.723 0.219 149.58)`, Icon: CircleCheck },
  cancelled: { label: `Cancelled`, color: `oklch(0.637 0.237 25.33)`, Icon: CircleX },
}

const PRIORITY: Record<PriorityKey, { label: string; color: string; Icon: typeof CircleIcon }> = {
  none: { label: `No priority`, color: `oklch(0.708 0 0)`, Icon: Minus },
  urgent: { label: `Urgent`, color: `oklch(0.637 0.237 25.33)`, Icon: AlertTriangle },
  high: { label: `High`, color: `oklch(0.705 0.213 47.6)`, Icon: SignalHigh },
  medium: { label: `Medium`, color: `oklch(0.795 0.184 86.05)`, Icon: SignalMedium },
  low: { label: `Low`, color: `oklch(0.623 0.214 259.85)`, Icon: SignalLow },
}

type LabelDot = { name: string; color: string }

interface Issue {
  ident: string
  title: string
  status: StatusKey
  priority: PriorityKey
  due?: string
  labels?: LabelDot[]
  assignee: string
}

export function MobileIssueDetail({
  issue,
  onBack,
}: {
  issue: Issue
  onBack: () => void
}) {
  const statusCfg = STATUS[issue.status]
  const StatusI = statusCfg.Icon
  const priCfg = PRIORITY[issue.priority]
  const PriI = priCfg.Icon

  return (
    <div className="md-detail">
      <div className="md-nav">
        <button className="md-back" onClick={onBack}>
          <ChevronLeft size={16} strokeWidth={2.2} />
          <span>Exponential</span>
        </button>
        <span className="md-ident-badge">{issue.ident}</span>
      </div>

      <div className="md-scroll">
        <h2 className="md-title">{issue.title}</h2>

        <div className="md-desc">
          <p>Add description&hellip;</p>
        </div>

        <div className="md-meta-section">
          <div className="md-meta-row">
            <span className="md-meta-label">Status</span>
            <span className="md-meta-pill" style={{ color: statusCfg.color }}>
              <StatusI size={12} strokeWidth={1.8} style={{ color: statusCfg.color }} />
              {statusCfg.label}
            </span>
          </div>
          <div className="md-meta-divider" />
          <div className="md-meta-row">
            <span className="md-meta-label">Priority</span>
            <span className="md-meta-pill" style={{ color: priCfg.color }}>
              <PriI size={12} strokeWidth={2} style={{ color: priCfg.color }} />
              {priCfg.label}
            </span>
          </div>
          <div className="md-meta-divider" />
          <div className="md-meta-row">
            <span className="md-meta-label">Assignee</span>
            <span className="md-meta-pill">
              <span className="md-assignee-av">{issue.assignee}</span>
              {issue.assignee === `D` ? `Danny` : `Niach`}
            </span>
          </div>
          {issue.due && (
            <>
              <div className="md-meta-divider" />
              <div className="md-meta-row">
                <span className="md-meta-label">Due date</span>
                <span className="md-meta-pill">
                  <Calendar size={11} strokeWidth={2} />
                  {issue.due}
                </span>
              </div>
            </>
          )}
        </div>

        {issue.labels && issue.labels.length > 0 && (
          <div className="md-labels-section">
            <span className="md-meta-label">Labels</span>
            <div className="md-labels-row">
              {issue.labels.map((l, i) => (
                <span key={i} className="md-label-pill">
                  <span className="md-label-dot" style={{ background: l.color }} />
                  {l.name}
                </span>
              ))}
            </div>
          </div>
        )}

        <div className="md-comments-head">
          <MessageSquare size={13} strokeWidth={2} />
          <span>Comments</span>
          <span className="md-comment-count">2</span>
        </div>

        <div className="md-comment">
          <span className="md-comment-avatar">D</span>
          <div className="md-comment-body">
            <div className="md-comment-meta">
              <strong>Danny</strong>
              <span>2h ago</span>
            </div>
            <p>Let&apos;s prioritize this for the next sprint.</p>
          </div>
        </div>

        <CodingSessionComment />
      </div>
    </div>
  )
}

function CodingSessionComment() {
  return (
    <div className="md-comment is-agent">
      <span className="md-comment-avatar is-bot">
        <Sparkles size={11} />
      </span>
      <div className="md-comment-body">
        <div className="md-comment-meta">
          <strong>Claude</strong>
          <span className="md-agent-badge">Live session</span>
          <span>now</span>
        </div>
        <div className="md-plan-card">
          <p>
            Opened a worktree on <code>exp/EXP-18</code>, proposed a plan, and
            started coding. Watch the terminal live &mdash; or type to steer.
          </p>
        </div>
        <div className="md-plan-actions">
          <button className="md-plan-btn is-primary">
            <Radio size={10} strokeWidth={2} />
            Watch &amp; steer
          </button>
        </div>
      </div>
    </div>
  )
}
