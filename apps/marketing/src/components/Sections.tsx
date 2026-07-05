import { useEffect, useRef, useState } from "react"
import { motion, AnimatePresence } from "motion/react"
import {
  AtSign,
  BellRing,
  CalendarDays,
  Check,
  FileText,
  GitPullRequest,
  Globe,
  Plug,
  Radio,
  Repeat,
  Terminal,
} from "lucide-react"
import {
  cardReveal,
  staggerContainer,
} from "../lib/animations"
import { LINKS } from "../lib/links"
import { IcBot, IcCopy, IcGithub, IcShield, IcZap } from "./icons"

export function ValueProps() {
  const cards = [
    {
      icon: <IcZap size={20} />,
      title: `Real-time, everywhere`,
      desc: `Every change syncs instantly to all five clients. No spinners, no stale lists, no pull-to-refresh.`,
    },
    {
      icon: <FileText size={20} strokeWidth={1.8} />,
      title: `One markdown, five clients`,
      desc: `GFM with byte-parity across web, iOS, Android, macOS and Linux. Task lists, images and code blocks round-trip everywhere.`,
    },
    {
      icon: <AtSign size={20} strokeWidth={1.8} />,
      title: `Mentions & comments`,
      desc: `@mention a teammate and they're notified and subscribed. Comments carry full markdown — issue links, task lists, code blocks, and images.`,
    },
    {
      icon: <BellRing size={20} strokeWidth={1.8} />,
      title: `Inbox & push`,
      desc: `Assignments, comments, mentions and PR updates land in your inbox — and on your phone as push notifications.`,
    },
    {
      icon: <Repeat size={20} strokeWidth={1.8} />,
      title: `Recurring issues`,
      desc: `Close a recurring issue and the next occurrence spawns itself. Daily, weekly, or monthly.`,
    },
    {
      icon: <IcGithub size={20} />,
      title: `GitHub, the App way`,
      desc: `One issue, one branch, one PR. The GitHub App mints scoped tokens — no personal tokens to manage — and merge detection closes the loop.`,
    },
    {
      icon: <CalendarDays size={20} strokeWidth={1.8} />,
      title: `Calendar sync`,
      desc: `Due dates appear in your Google Calendar automatically and disappear when the issue is done.`,
    },
    {
      icon: <Plug size={20} strokeWidth={1.8} />,
      title: `MCP built in`,
      desc: `Point Claude Code, Cursor, or any MCP client at /api/mcp and let it drive issues, labels and comments.`,
    },
    {
      icon: <Globe size={20} strokeWidth={1.8} />,
      title: `Public workspaces`,
      desc: `Open a workspace to the world — read-only or contributor mode. Perfect for public roadmaps and feedback.`,
    },
    {
      icon: <IcShield size={20} />,
      title: `Keyboard-first`,
      desc: `Inline edits, context menus, save-on-blur. Built to keep your hands on the keys and out of modals.`,
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
        A real-time issue tracker you can self-host. Read the source, fork it,
        run it on your own infrastructure.
      </p>

      <a className="btn btn-primary repo-cta" href={LINKS.github.repo}>
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

type CodingPhase = `idle` | `plan` | `coding` | `pr`

const CODING_SEQUENCE: Record<CodingPhase, [CodingPhase, number]> = {
  idle: [`plan`, 1000],
  plan: [`coding`, 3200],
  coding: [`pr`, 1800],
  pr: [`idle`, 4200],
}

export function AgentTimeline() {
  const [phase, setPhase] = useState<CodingPhase>(`idle`)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    const [to, delay] = CODING_SEQUENCE[phase]
    timerRef.current = setTimeout(() => setPhase(to), delay)
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [phase])

  const startNow = () => {
    if (timerRef.current) clearTimeout(timerRef.current)
    setPhase(`plan`)
  }

  return (
    <div className="agent-stage-wrap">
      <SteerSessionPreview />
      <div className="agent-timeline">
        <div className="agent-tl-issue">
          <span className="agent-tl-issue-ident">EXP-181</span>
          <span className="agent-tl-issue-title">
            Add webhook events for issue mutations
          </span>
        </div>

        <AnimatePresence mode="popLayout">
          {phase === `idle` && (
            <motion.div
              key="start"
              className="agent-tl-start"
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.35, ease: `easeOut` }}
            >
              <button
                className="agent-tl-btn is-primary"
                onClick={startNow}
                type="button"
              >
                <Terminal size={11} strokeWidth={2.2} /> Start coding
              </button>
              <span className="agent-tl-start-hint">
                Opens Claude in the desktop terminal
              </span>
            </motion.div>
          )}

          {(phase === `plan` || phase === `coding` || phase === `pr`) && (
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
                <span className="agent-tl-time">worktree exp/EXP-181</span>
                <span className="agent-tl-tag">Terminal</span>
              </div>
              <div className="agent-tl-card-body">
                Plan: add a <code>webhooks</code> table and a{` `}
                <code>dispatchWebhook()</code> helper, fired on issue create,
                update, and delete. Proceeding &mdash; tell me to adjust.
              </div>
              {phase === `plan` && (
                <div
                  className="agent-tl-prompt"
                  style={{
                    marginTop: 10,
                    display: `flex`,
                    alignItems: `center`,
                    gap: 8,
                    fontFamily: `var(--font-mono)`,
                    fontSize: 11.5,
                    color: `rgba(255,255,255,0.72)`,
                  }}
                >
                  <span style={{ color: `var(--accent)` }}>›</span>
                  <span>go &mdash; but sign payloads with HMAC-SHA256</span>
                  <span className="caret" aria-hidden />
                </div>
              )}
            </motion.div>
          )}

          {(phase === `coding` || phase === `pr`) && (
            <motion.div
              key="coding"
              className="agent-tl-activity"
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.35, ease: `easeOut`, delay: 0.15 }}
            >
              {phase === `coding` ? (
                <>
                  <span className="agent-tl-spinner" aria-hidden />
                  <span>Coding &mdash; committing &amp; pushing exp/EXP-181&hellip;</span>
                </>
              ) : (
                <>
                  <Check size={12} strokeWidth={2.4} style={{ color: `oklch(0.78 0.15 155)` }} />
                  <span>Pushed exp/EXP-181</span>
                </>
              )}
            </motion.div>
          )}

          {phase === `pr` && (
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
                Claude opened PR <strong>niach/exponential#214</strong> &middot;
                &ldquo;Add webhook events&rdquo;
              </span>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  )
}

function SteerSessionPreview() {
  return (
    <div className="ws-members-preview">
      <div className="ws-members-head">
        <span className="ws-members-title">Live coding session</span>
        <span className="ws-members-count">exp/EXP-181</span>
      </div>
      <div className="ws-member-row">
        <span
          className="ws-member-avatar"
          style={{ background: `rgba(255,255,255,0.1)`, color: `#fff` }}
        >
          D
        </span>
        <div className="ws-member-info">
          <span className="ws-member-name">
            Danny
            <span className="ws-member-online" />
          </span>
          <span className="ws-member-email">
            running on Danny&apos;s MacBook Pro
          </span>
        </div>
        <span className="ws-member-badge">Desktop</span>
      </div>
      <div className="ws-member-row">
        <span
          className="ws-member-avatar"
          style={{
            background: `color-mix(in oklch, oklch(0.65 0.22 280) 25%, transparent)`,
            color: `oklch(0.65 0.22 280)`,
          }}
        >
          <Radio size={12} strokeWidth={2} />
        </span>
        <div className="ws-member-info">
          <span className="ws-member-name">
            iPhone
            <span className="ws-member-online" />
          </span>
          <span className="ws-member-email">watching &amp; steering</span>
        </div>
        <span className="ws-member-badge is-agent">Live</span>
      </div>
    </div>
  )
}
