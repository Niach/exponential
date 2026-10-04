import { ClaudeIcon, CodexIcon } from "./brand-icons"
import { ClaudeSpinner } from "./claude-spinner"
import { conceptIcon } from "./icons.generated"
import { cn } from "./cn"

// EXP-850 §5: the running agent's BRAND mark, beside the working caption —
// the same two marks the desktop IDE and the iOS session screen draw
// (`apps/desktop/assets/icons/{claude,codex}.svg`,
// `Assets.xcassets/agent-{claude,codex}`), inline paths because the web app
// ships no brand assets of its own (both live in `brand-icons.tsx`, the ONE
// copy the pickers draw too, EXP-877). Brand marks are deliberately NOT
// part of the Lucide concept registry (they are not glyphs we may restyle);
// every OTHER glyph on this surface is a concept, and an agent this build
// does not know falls back to the `settings-agents` concept exactly like the
// desktop does.
const SettingsAgentsIcon = conceptIcon(`settings-agents`)

export function AgentBrandMark({
  agent,
  className,
  pulse = false,
}: {
  /** The run's `coding_sessions.agent` (`claude`, `codex`, an external id or
   *  null — a row without one is a claude run). */
  agent: string | null | undefined
  className?: string
  /** EXP-850 §5: the working beat — opacity 0.4 to 1 over 1.4s, behind
   *  `motion-safe:` so reduced motion leaves it steady. */
  pulse?: boolean
}) {
  const id = (agent ?? `claude`).trim().toLowerCase()
  const shared = cn(
    `size-3 shrink-0`,
    pulse && `motion-safe:animate-agent-pulse`,
    className
  )
  if (id === `codex`) {
    return <CodexIcon className={shared} />
  }
  if (id === `` || id === `claude`) {
    return <ClaudeIcon className={shared} />
  }
  // An external ACP agent (EXP-849): no brand mark exists, so the generic
  // agents concept stands in.
  return <SettingsAgentsIcon className={shared} />
}

/** EXP-1184: what a live run is doing, the ×4 rule
 *  (`fixtures/session-display.json`): it waits on you, it works, its PR is
 *  open, or it is done (no open PR, or merged). */
export type RunMarkState = `needs_input` | `working` | `review` | `done`

/** The badge each parked state wears, in the session-dot palette. */
export const RUN_MARK_BADGE_CLASS: Record<
  Exclude<RunMarkState, `working`>,
  string
> = {
  needs_input: `bg-amber-500`,
  review: `bg-emerald-500`,
  done: `bg-sky-500`,
}

/** EXP-1184: the agent at work — Claude's own frame-stepped spark, or the
 *  EXP-850 pulse for an agent with no working art of its own. */
export function AgentWorkingMark({
  agent,
  className,
}: {
  agent: string | null | undefined
  className?: string
}) {
  const id = (agent ?? `claude`).trim().toLowerCase()
  if (id === `` || id === `claude`) {
    return <ClaudeSpinner className={cn(`size-3`, className)} />
  }
  return <AgentBrandMark agent={agent} className={className} pulse />
}

/**
 * EXP-923 / EXP-1162 / EXP-1184: a LIVE run's mark — wherever a run is named
 * (the sidebar's Running rows, the compact rail, the Work face strip's Run
 * tab) it reads the same: the agent's working mark while it works, else its
 * brand mark with a small state badge (amber: wants you, emerald: PR open,
 * sky: done). No `state` = a paused run, the bare mark.
 */
export function AgentRunMark({
  agent,
  state,
  needsYou = false,
  ringClassName = `ring-sidebar`,
  className,
}: {
  agent: string | null | undefined
  state?: RunMarkState
  /** EXP-1068/1082 §4: an open question for a person — the RED badge, which
   *  beats every state badge. */
  needsYou?: boolean
  /** The badge's ring is the ground it sits on. */
  ringClassName?: string
  className?: string
}) {
  const badge = needsYou
    ? `bg-red-500`
    : state && state !== `working`
      ? RUN_MARK_BADGE_CLASS[state]
      : null
  return (
    <span
      data-state={state}
      className={cn(`relative flex size-3.5 items-center justify-center`, className)}
    >
      {state === `working` ? (
        <AgentWorkingMark agent={agent} className="size-3.5" />
      ) : (
        <AgentBrandMark agent={agent} className="size-3.5" />
      )}
      {badge && (
        <span
          aria-hidden
          data-slot="run-mark-badge"
          className={cn(
            `absolute -top-0.5 -right-0.5 size-1.5 rounded-full ring-2`,
            ringClassName,
            badge
          )}
        />
      )}
    </span>
  )
}
