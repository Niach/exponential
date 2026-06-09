import { useEffect, useRef, useState } from "react"
import { useInView, useReducedMotion } from "motion/react"

export type AgentPhase =
  | `idle`
  | `assigned`
  | `plan`
  | `waiting`
  | `approved`
  | `working`
  | `pr`
  | `done`

type LineClass = `dim` | `cmd` | `out` | `plan` | `tool` | `ok`

export type TermStep =
  | { kind: `print`; text: string; cls?: LineClass; ms?: number }
  | { kind: `type`; text: string; cls?: LineClass; cps?: number; ms?: number }
  | { kind: `pause`; ms: number }
  | { kind: `phase`; phase: AgentPhase }
  | { kind: `clear` }

type Line = { text: string; cls?: LineClass }

const MAX_LINES = 16

/* The full agent run, choreographed with the hero sidecar. */
export const heroScript: TermStep[] = [
  { kind: `pause`, ms: 500 },
  { kind: `phase`, phase: `assigned` },
  { kind: `type`, text: `claude --session exp-181`, cls: `cmd`, ms: 500 },
  { kind: `print`, text: `[exponential] EXP-181 assigned — reading issue…`, cls: `dim`, ms: 1300 },
  { kind: `print`, text: `[exponential] worktree ready · branch exp-181-webhooks`, cls: `dim`, ms: 1100 },
  { kind: `phase`, phase: `plan` },
  { kind: `print`, text: `◆ Plan — Add webhook events for issue mutations`, cls: `plan`, ms: 550 },
  { kind: `print`, text: `  1. webhooks table + Drizzle migration`, cls: `out`, ms: 450 },
  { kind: `print`, text: `  2. dispatchWebhook() on create / update / delete`, cls: `out`, ms: 450 },
  { kind: `print`, text: `  3. HMAC-SHA256 payload signing`, cls: `out`, ms: 450 },
  { kind: `print`, text: `  4. tests for retry + signature`, cls: `out`, ms: 600 },
  { kind: `phase`, phase: `waiting` },
  { kind: `print`, text: `[exponential] plan posted — waiting for approval…`, cls: `dim`, ms: 200 },
  { kind: `pause`, ms: 2400 },
  { kind: `phase`, phase: `approved` },
  { kind: `print`, text: `✓ Plan approved by danny`, cls: `ok`, ms: 700 },
  { kind: `phase`, phase: `working` },
  { kind: `print`, text: `▸ Edit src/db/schema.ts`, cls: `tool`, ms: 750 },
  { kind: `print`, text: `▸ Edit src/lib/webhooks.ts`, cls: `tool`, ms: 950 },
  { kind: `print`, text: `▸ Bash bun run migrate:generate`, cls: `tool`, ms: 800 },
  { kind: `print`, text: `  → 0042_webhooks.sql`, cls: `out`, ms: 650 },
  { kind: `print`, text: `▸ Bash bun test`, cls: `tool`, ms: 1100 },
  { kind: `print`, text: `  ✓ 24 tests passed`, cls: `ok`, ms: 850 },
  { kind: `print`, text: `[exponential] commit · push exp-181-webhooks`, cls: `dim`, ms: 1100 },
  { kind: `phase`, phase: `pr` },
  { kind: `print`, text: `✓ Opened PR niach/exponential#214 — Add webhook events`, cls: `ok`, ms: 400 },
  { kind: `phase`, phase: `done` },
  { kind: `pause`, ms: 3600 },
  { kind: `phase`, phase: `idle` },
  { kind: `clear` },
]

/* Short non-looping flavour for sections / docs. */
export const snippetScript: TermStep[] = [
  { kind: `pause`, ms: 300 },
  { kind: `type`, text: `claude --session exp-204`, cls: `cmd`, ms: 400 },
  { kind: `print`, text: `[exponential] EXP-204 assigned — reading issue…`, cls: `dim`, ms: 900 },
  { kind: `print`, text: `▸ Edit src/routes/inbox.tsx`, cls: `tool`, ms: 800 },
  { kind: `print`, text: `▸ Bash bun test`, cls: `tool`, ms: 900 },
  { kind: `print`, text: `  ✓ 12 tests passed`, cls: `ok`, ms: 800 },
  { kind: `print`, text: `✓ Opened PR niach/exponential#231`, cls: `ok`, ms: 400 },
]

function staticFrame(script: TermStep[]): Line[] {
  const lines: Line[] = []
  for (const step of script) {
    if (step.kind === `clear`) lines.length = 0
    if (step.kind === `print` || step.kind === `type`)
      lines.push({ text: step.text, cls: step.cls })
  }
  return lines.slice(-MAX_LINES)
}

export function TerminalDemo({
  script = heroScript,
  title = `claude — EXP-181 · worktree`,
  height = 340,
  loop = true,
  onPhase,
}: {
  script?: TermStep[]
  title?: string
  height?: number
  loop?: boolean
  onPhase?: (phase: AgentPhase) => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  const inView = useInView(ref, { amount: 0.25 })
  const reduced = useReducedMotion()
  const [lines, setLines] = useState<Line[]>([])
  const [typing, setTyping] = useState<Line | null>(null)

  useEffect(() => {
    if (reduced) {
      setLines(staticFrame(script))
      onPhase?.(`done`)
      return
    }
    if (!inView) return

    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | null = null
    let i = 0

    setLines([])
    setTyping(null)

    const schedule = (fn: () => void, ms: number) => {
      timer = setTimeout(fn, ms)
    }

    const exec = () => {
      if (cancelled) return
      if (i >= script.length) {
        if (loop) {
          i = 0
          schedule(exec, 700)
        }
        return
      }
      const step = script[i++]
      switch (step.kind) {
        case `phase`:
          onPhase?.(step.phase)
          exec()
          break
        case `clear`:
          setLines([])
          setTyping(null)
          schedule(exec, 300)
          break
        case `pause`:
          schedule(exec, step.ms)
          break
        case `print`:
          setLines((ls) =>
            [...ls, { text: step.text, cls: step.cls }].slice(-MAX_LINES),
          )
          schedule(exec, step.ms ?? 90)
          break
        case `type`: {
          const total = step.text.length
          const interval = Math.max(20, 2000 / (step.cps ?? 28))
          let j = 0
          const tick = () => {
            if (cancelled) return
            j = Math.min(total, j + 2)
            setTyping({ text: step.text.slice(0, j), cls: step.cls })
            if (j < total) {
              schedule(tick, interval)
            } else {
              setTyping(null)
              setLines((ls) =>
                [...ls, { text: step.text, cls: step.cls }].slice(-MAX_LINES),
              )
              schedule(exec, step.ms ?? 120)
            }
          }
          tick()
          break
        }
      }
    }

    exec()

    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
      onPhase?.(`idle`)
    }
  }, [inView, reduced, script, loop])

  const prompt = typing ?? { text: ``, cls: `cmd` as const }

  return (
    <div ref={ref} className="term-window" role="img" aria-label="Terminal demo of a coding agent working on an issue and opening a pull request">
      <div className="term-bar">
        <div className="term-bar-dots" aria-hidden>
          <span />
          <span />
          <span />
        </div>
        <span className="term-bar-title">
          <strong>{title}</strong>
        </span>
      </div>
      <div className="term-body" style={{ "--term-h": `${height}px` } as React.CSSProperties}>
        {lines.map((l, idx) => (
          <div key={idx} className={`term-line${l.cls ? ` tl-${l.cls}` : ``}`}>
            {l.text}
          </div>
        ))}
        <div className={`term-line${prompt.cls ? ` tl-${prompt.cls}` : ``}`}>
          {prompt.text}
          {!reduced && <span className="caret" aria-hidden />}
        </div>
      </div>
    </div>
  )
}
