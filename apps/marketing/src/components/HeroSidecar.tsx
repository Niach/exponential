import { motion, AnimatePresence } from "motion/react"
import { BellRing, Check, GitPullRequest, MessageSquareText, X } from "lucide-react"
import type { AgentPhase } from "./TerminalDemo"
import { IcBot } from "./icons"

const PHASE_ORDER: AgentPhase[] = [
  `idle`,
  `assigned`,
  `plan`,
  `waiting`,
  `approved`,
  `working`,
  `pr`,
  `done`,
]

function atLeast(phase: AgentPhase, min: AgentPhase) {
  return PHASE_ORDER.indexOf(phase) >= PHASE_ORDER.indexOf(min)
}

export function HeroSidecar({ phase }: { phase: AgentPhase }) {
  const status = atLeast(phase, `pr`)
    ? { label: `Done`, cls: `is-done` }
    : atLeast(phase, `approved`)
      ? { label: `In Progress`, cls: `is-progress` }
      : { label: `Todo`, cls: `` }

  return (
    <div className="hero-sidecar">
      <div className="sidecar-issue">
        <div className="sidecar-issue-head">
          <span>EXP-181</span>
          <span className={`sidecar-status ${status.cls}`}>
            <span className="dot" aria-hidden /> {status.label}
          </span>
        </div>
        <span className="sidecar-issue-title">
          Add webhook events for issue mutations
        </span>
        <AnimatePresence>
          {atLeast(phase, `assigned`) && (
            <motion.span
              key="assignee"
              className="sidecar-assignee"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            >
              <span className="agent-tl-avatar is-agent">
                <IcBot size={11} />
              </span>
              Assigned to Claude · Agent
            </motion.span>
          )}
        </AnimatePresence>
      </div>

      <AnimatePresence mode="popLayout">
        {atLeast(phase, `plan`) && !atLeast(phase, `pr`) && (
          <motion.div
            key="plan"
            className="agent-tl-card is-plan"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.3, ease: `easeOut` }}
          >
            <div className="agent-tl-card-head">
              <span className="agent-tl-avatar is-agent">
                <IcBot size={11} />
              </span>
              <span className="agent-tl-author">Claude</span>
              <span className="agent-tl-time">just now</span>
              <span className="agent-tl-tag">Plan</span>
            </div>
            <div className="agent-tl-card-body">
              Add a <code>webhooks</code> table and a{` `}
              <code>dispatchWebhook()</code> helper. Sign payloads with
              HMAC-SHA256.
            </div>
            {(phase === `plan` || phase === `waiting`) && (
              <div className="agent-tl-actions">
                <button className="agent-tl-btn is-primary">
                  <Check size={11} strokeWidth={2.4} /> Approve
                </button>
                <button className="agent-tl-btn">
                  <MessageSquareText size={11} strokeWidth={2} /> Request changes
                </button>
                <button className="agent-tl-btn">
                  <X size={11} strokeWidth={2.2} /> Cancel
                </button>
              </div>
            )}
            {atLeast(phase, `approved`) && (
              <div className="agent-tl-activity" style={{ border: 0, padding: `4px 0 0` }}>
                <Check size={12} strokeWidth={2.4} style={{ color: `var(--phosphor)` }} />
                <span>Approved by danny</span>
              </div>
            )}
          </motion.div>
        )}

        {(phase === `waiting`) && (
          <motion.div
            key="toast"
            className="sidecar-toast"
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.25 }}
          >
            <BellRing size={13} strokeWidth={2} />
            <span>Plan ready — review on any device</span>
          </motion.div>
        )}

        {atLeast(phase, `pr`) && (
          <motion.div
            key="pr"
            className="sidecar-pr"
            initial={{ opacity: 0, y: 12, filter: `brightness(1.6)` }}
            animate={{ opacity: 1, y: 0, filter: `brightness(1)` }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.45, ease: `easeOut` }}
          >
            <span className="sidecar-pr-icon">
              <GitPullRequest size={12} strokeWidth={2.2} />
            </span>
            <span>
              Opened PR <strong>niach/exponential#214</strong>
              <br />
              <span style={{ color: `var(--fg-muted)`, fontSize: 11.5 }}>
                Add webhook events · 6 files
              </span>
            </span>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
