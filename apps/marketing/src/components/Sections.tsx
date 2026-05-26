import { useEffect, useRef, useState } from "react"
import { motion, AnimatePresence } from "motion/react"
import { Check, GitPullRequest, MessageSquareText, X } from "lucide-react"
import {
  cardReveal,
  staggerContainer,
} from "../lib/animations"
import { IcBot, IcCopy, IcGithub, IcShield, IcZap } from "./icons"

export function ValueProps() {
  const cards = [
    {
      icon: <IcZap size={22} />,
      title: `Real-time, everywhere`,
      desc: `Electric streams every change to every client. Edits apply locally and reconcile through Postgres — no spinners, no stale lists.`,
    },
    {
      icon: <IcShield size={22} />,
      title: `Native on every device`,
      desc: `SwiftUI on iOS, Compose on Android. Offline-first, multi-server, live sync. Your tracker in your pocket.`,
    },
    {
      icon: <IcBot size={22} />,
      title: `AI agents that ship`,
      desc: `Assign an issue to Claude or Codex. They plan in comments, you approve, a PR opens on GitHub.`,
    },
  ]

  return (
    <motion.div
      className="value-props"
      variants={staggerContainer}
      initial="hidden"
      whileInView="visible"
      viewport={{ once: true, amount: 0.2 }}
    >
      {cards.map((c) => (
        <motion.div key={c.title} className="value-card" variants={cardReveal}>
          <span className="value-card-icon">{c.icon}</span>
          <h3>{c.title}</h3>
          <p>{c.desc}</p>
        </motion.div>
      ))}
    </motion.div>
  )
}

export function RepoCard() {
  return (
    <div className="repo-card">
      <div className="repo-head">
        <IcGithub size={18} />
        <span className="repo-owner">Niach</span>
        <span style={{ color: `var(--fg-dim)` }}>/</span>
        <span className="repo-name">exponential</span>
      </div>

      <div className="repo-meta">
        <span>
          <IcShield size={12} /> ELv2 license
        </span>
        <span>v0.13.0</span>
      </div>

      <p
        style={{
          margin: 0,
          fontSize: 13.5,
          color: `var(--fg-muted)`,
          lineHeight: 1.6,
        }}
      >
        A real-time issue tracker built with TanStack Start, Electric SQL,
        Drizzle, and Better Auth. Read the source, fork it, run it on your own
        infrastructure.
      </p>

      <a
        className="btn btn-primary repo-cta"
        href="https://github.com/Niach/exponential"
      >
        <IcGithub size={14} /> View on GitHub
      </a>
    </div>
  )
}

export function CopyBlock() {
  const [copied, setCopied] = useState(false)
  const cmd = `git clone https://github.com/Niach/exponential && cd exponential && docker compose up -d`
  const onCopy = () => {
    navigator.clipboard?.writeText(cmd)
    setCopied(true)
    setTimeout(() => setCopied(false), 1400)
  }
  return (
    <div className="copy-cmd">
      <code>{cmd}</code>
      <button className="copy-btn" onClick={onCopy}>
        <IcCopy size={12} /> {copied ? `Copied` : `Copy`}
      </button>
    </div>
  )
}

type TimelinePhase = `idle` | `plan` | `working` | `done`

export function AgentTimeline() {
  const [phase, setPhase] = useState<TimelinePhase>(`idle`)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [userApproved, setUserApproved] = useState(false)

  const startSequence = () => {
    setPhase(`idle`)
    setUserApproved(false)
    timerRef.current = setTimeout(() => setPhase(`plan`), 600)
  }

  useEffect(() => {
    startSequence()
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [])

  const handleApprove = () => {
    if (phase !== `plan`) return
    setUserApproved(true)
    setPhase(`working`)
    timerRef.current = setTimeout(() => {
      setPhase(`done`)
      timerRef.current = setTimeout(() => startSequence(), 4000)
    }, 2000)
  }

  useEffect(() => {
    if (phase === `plan` && !userApproved) {
      timerRef.current = setTimeout(() => {
        handleApprove()
      }, 3000)
    }
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [phase, userApproved])

  return (
    <div className="agent-stage-wrap">
      <WorkspaceMembersPreview />
      <div className="agent-timeline">
        <div className="agent-tl-issue">
          <span className="agent-tl-issue-ident">EXP-181</span>
          <span className="agent-tl-issue-title">
            Add webhook events for issue mutations
          </span>
        </div>

        <AnimatePresence mode="popLayout">
          {(phase === `plan` || phase === `working` || phase === `done`) && (
            <motion.div
              key="plan"
              className="agent-tl-card is-plan"
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.35, ease: `easeOut` }}
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
                <code>dispatchWebhook()</code> helper. Fire on issue create,
                update, and delete. Sign payloads with HMAC-SHA256.
              </div>
              {phase === `plan` && (
                <div className="agent-tl-actions">
                  <button
                    className="agent-tl-btn is-primary"
                    onClick={handleApprove}
                  >
                    <Check size={11} strokeWidth={2.4} /> Approve
                  </button>
                  <button className="agent-tl-btn">
                    <MessageSquareText size={11} strokeWidth={2} /> Request
                    changes
                  </button>
                  <button className="agent-tl-btn">
                    <X size={11} strokeWidth={2.2} /> Cancel
                  </button>
                </div>
              )}
            </motion.div>
          )}

          {(phase === `working` || phase === `done`) && (
            <motion.div
              key="working"
              className="agent-tl-activity"
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.35, ease: `easeOut`, delay: 0.15 }}
            >
              {phase === `working` ? (
                <>
                  <span className="agent-tl-spinner" aria-hidden />
                  <span>Agent is working &mdash; creating worktree&hellip;</span>
                </>
              ) : (
                <>
                  <Check size={12} strokeWidth={2.4} style={{ color: `oklch(0.78 0.15 155)` }} />
                  <span>Worktree created, writing code&hellip;</span>
                </>
              )}
            </motion.div>
          )}

          {phase === `done` && (
            <motion.div
              key="pr"
              className="agent-tl-pr"
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.35, ease: `easeOut`, delay: 0.15 }}
            >
              <span className="agent-tl-pr-icon">
                <GitPullRequest size={11} strokeWidth={2.2} />
              </span>
              <span>
                Opened PR <strong>niach/exponential#214</strong> &middot;
                &ldquo;Add webhook events&rdquo;
              </span>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  )
}

function WorkspaceMembersPreview() {
  const members = [
    {
      name: `Danny`,
      email: `danny@acme.io`,
      role: `Admin`,
      isAgent: false,
      online: true,
      color: `oklch(0.62 0.18 280)`,
    },
    {
      name: `Claude`,
      email: `claude@agents`,
      role: `Agent`,
      isAgent: true,
      online: true,
      color: `oklch(0.65 0.22 280)`,
    },
    {
      name: `Codex`,
      email: `codex@agents`,
      role: `Agent`,
      isAgent: true,
      online: false,
      color: `oklch(0.72 0.15 155)`,
    },
  ]

  return (
    <div className="ws-members-preview">
      <div className="ws-members-head">
        <span className="ws-members-title">Workspace members</span>
        <span className="ws-members-count">{members.length}</span>
      </div>
      {members.map((m) => (
        <div key={m.name} className="ws-member-row">
          <span
            className="ws-member-avatar"
            style={{
              background: m.isAgent
                ? `color-mix(in oklch, ${m.color} 25%, transparent)`
                : `rgba(255,255,255,0.1)`,
              color: m.isAgent ? m.color : `#fff`,
            }}
          >
            {m.isAgent ? <IcBot size={11} /> : m.name[0]}
          </span>
          <div className="ws-member-info">
            <span className="ws-member-name">
              {m.name}
              {m.online && <span className="ws-member-online" />}
            </span>
            <span className="ws-member-email">{m.email}</span>
          </div>
          <span
            className={`ws-member-badge ${m.isAgent ? `is-agent` : ``}`}
          >
            {m.role}
          </span>
        </div>
      ))}
    </div>
  )
}
