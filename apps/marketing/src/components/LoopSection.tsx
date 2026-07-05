import { useEffect, useRef, useState, type ReactNode } from "react"
import { motion, AnimatePresence, useInView, useReducedMotion } from "motion/react"
import {
  Check,
  GitMerge,
  Mail,
  MessageSquareText,
  MousePointerClick,
} from "lucide-react"
import { IcBot, IcCircle } from "./icons"

type StageKey = `capture` | `issue` | `code` | `pr` | `email`

type Stage = {
  key: StageKey
  label: string
  icon: ReactNode
  ms: number
}

/* The full loop, choreographed like the hero terminal: a driver advances
   through the five stages; each stage's detail panel reacts. */
const STAGES: Stage[] = [
  { key: `capture`, label: `Feedback captured`, icon: <MessageSquareText size={15} strokeWidth={2} />, ms: 3400 },
  { key: `issue`, label: `Issue created`, icon: <IcCircle size={15} stroke={2} />, ms: 2600 },
  { key: `code`, label: `Claude codes`, icon: <IcBot size={15} />, ms: 3800 },
  { key: `pr`, label: `PR ships`, icon: <GitMerge size={15} strokeWidth={2} />, ms: 2600 },
  { key: `email`, label: `Reporter emailed`, icon: <Mail size={15} strokeWidth={2} />, ms: 3400 },
]

export function LoopSection() {
  const ref = useRef<HTMLDivElement>(null)
  const inView = useInView(ref, { amount: 0.3 })
  const reduced = useReducedMotion()
  const [active, setActive] = useState(0)

  useEffect(() => {
    if (reduced) {
      setActive(STAGES.length - 1)
      return
    }
    if (!inView) return

    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | null = null
    let i = 0
    setActive(0)

    const tick = () => {
      if (cancelled) return
      timer = setTimeout(() => {
        i = (i + 1) % STAGES.length
        setActive(i)
        tick()
      }, STAGES[i].ms)
    }
    tick()

    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
    }
  }, [inView, reduced])

  const activeKey = STAGES[active].key

  return (
    <section id="loop">
      <div className="shell">
        <div className="loop-head">
          <span className="section-eyebrow">The loop</span>
          <h2 className="section-title">One report, all the way to shipped.</h2>
          <p className="section-sub" style={{ margin: `0 auto` }}>
            A user hits a bug and taps your feedback widget. It becomes an issue,
            Claude writes the fix, the PR merges &mdash; and the person who
            reported it gets an email saying it&apos;s live. The whole loop, on
            your infrastructure.
          </p>
        </div>

        <div className="loop-stage" ref={ref}>
          <div className="loop-rail" role="list">
            {STAGES.map((s, idx) => (
              <div key={s.key} className="loop-rail-item" role="listitem">
                <span
                  className={`loop-node${idx === active ? ` is-active` : ``}${
                    idx < active ? ` is-past` : ``
                  }`}
                >
                  {s.icon}
                </span>
                <span
                  className={`loop-node-label${
                    idx === active ? ` is-active` : ``
                  }`}
                >
                  {s.label}
                </span>
                {idx < STAGES.length - 1 && (
                  <span
                    className={`loop-connector${
                      idx < active ? ` is-filled` : ``
                    }`}
                    aria-hidden
                  />
                )}
              </div>
            ))}
            <span className="loop-return" aria-hidden>
              <svg viewBox="0 0 120 40" width="120" height="40" fill="none">
                <path
                  d="M118 6 C118 30 90 34 60 34 C30 34 4 30 4 8"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeDasharray="4 5"
                />
                <path
                  d="M9 14 L4 7 L-1 14"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  transform="translate(0 -1)"
                />
              </svg>
              <span className="loop-return-label">and around again</span>
            </span>
          </div>

          <div className="loop-detail">
            <AnimatePresence mode="wait">
              <motion.div
                key={activeKey}
                className="loop-detail-panel"
                initial={reduced ? false : { opacity: 0, y: 12 }}
                animate={{ opacity: 1, y: 0 }}
                exit={reduced ? undefined : { opacity: 0, y: -10 }}
                transition={{ duration: 0.35, ease: `easeOut` }}
              >
                <StageDetail stage={activeKey} />
              </motion.div>
            </AnimatePresence>
          </div>
        </div>
      </div>
    </section>
  )
}

function StageDetail({ stage }: { stage: StageKey }) {
  if (stage === `capture`) {
    return (
      <div className="loop-card loop-widget">
        <div className="loop-widget-head">
          <MessageSquareText size={14} strokeWidth={2} />
          <span>Send us feedback</span>
        </div>
        <div className="loop-widget-shot">
          <span className="loop-widget-annot" />
          <span className="loop-widget-cursor">
            <MousePointerClick size={16} strokeWidth={2} />
          </span>
        </div>
        <p className="loop-widget-text">
          Checkout button overlaps the total on mobile.
        </p>
        <div className="loop-widget-foot">
          <span className="loop-chip">sam@acme.io</span>
          <span className="loop-widget-send">Send</span>
        </div>
      </div>
    )
  }

  if (stage === `issue`) {
    return (
      <div className="loop-card loop-issue">
        <span className="loop-issue-status">
          <IcCircle size={13} stroke={2} />
        </span>
        <span className="loop-issue-ident">EXP-482</span>
        <span className="loop-issue-title">
          Checkout button overlaps total on mobile
        </span>
        <span className="loop-issue-reporter">from sam@acme.io</span>
      </div>
    )
  }

  if (stage === `code`) {
    return (
      <div className="loop-card loop-term">
        <div className="loop-term-bar">
          <span className="loop-term-dots" aria-hidden>
            <span />
            <span />
            <span />
          </span>
          <span className="loop-term-title">claude — EXP-482 · worktree</span>
        </div>
        <div className="loop-term-body">
          <div className="tl-dim">[exponential] EXP-482 assigned — reading issue…</div>
          <div className="tl-plan">◆ Fix checkout button overflow on mobile</div>
          <div className="tl-tool">▸ Edit src/routes/checkout.tsx</div>
          <div className="tl-tool">▸ Bash bun test</div>
          <div className="tl-ok">  ✓ 18 tests passed</div>
          <div className="tl-dim">[exponential] commit · push exp-482</div>
        </div>
      </div>
    )
  }

  if (stage === `pr`) {
    return (
      <div className="loop-card loop-pr">
        <span className="loop-pr-icon">
          <GitMerge size={16} strokeWidth={2} />
        </span>
        <div className="loop-pr-body">
          <span className="loop-pr-line">
            Merged <strong>niach/exponential#214</strong>
          </span>
          <span className="loop-pr-sub">
            Fix checkout button overflow · 3 files · auto-closes EXP-482
          </span>
        </div>
        <span className="loop-pr-badge">
          <Check size={12} strokeWidth={2.6} /> Merged
        </span>
      </div>
    )
  }

  return (
    <div className="loop-card loop-email">
      <div className="loop-email-head">
        <span className="loop-email-avatar">
          <Mail size={14} strokeWidth={2} />
        </span>
        <div className="loop-email-meta">
          <span className="loop-email-to">To sam@acme.io</span>
          <span className="loop-email-subject">
            Your feedback shipped — it&apos;s fixed
          </span>
        </div>
      </div>
      <p className="loop-email-body">
        The issue you reported (checkout button on mobile) is resolved and live.
        Thanks for helping us make it exponential.
      </p>
    </div>
  )
}
