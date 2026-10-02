import { ClaudeIcon, CodexIcon } from "./brand-icons"
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

/**
 * EXP-923 / EXP-1162: a RUNNING run's mark — the agent's brand mark with the
 * small "wants you" badge. One component for the sidebar's Running rows, the
 * compact rail and the Work face strip's Run tab, so a live run reads the
 * same wherever it is named (contract `detail-chrome.json` FACE MARKS, ×4).
 */
export function AgentRunMark({
  agent,
  needsInput = false,
  needsYou = false,
  pulse = false,
  ringClassName = `ring-sidebar`,
  className,
}: {
  agent: string | null | undefined
  /** The run waits on a person: the amber badge. */
  needsInput?: boolean
  /** EXP-1068/1082 §4: an open question for a person — the RED badge, which
   *  beats the amber one. */
  needsYou?: boolean
  /** The agent is mid-turn: the session screen's working beat. */
  pulse?: boolean
  /** The badge's ring is the ground it sits on. */
  ringClassName?: string
  className?: string
}) {
  return (
    <span
      className={cn(`relative flex size-3.5 items-center justify-center`, className)}
    >
      <AgentBrandMark agent={agent} className="size-3.5" pulse={pulse} />
      {(needsInput || needsYou) && (
        <span
          aria-hidden
          className={cn(
            `absolute -top-0.5 -right-0.5 size-1.5 rounded-full ring-2`,
            ringClassName,
            needsYou ? `bg-red-500` : `bg-yellow-400`
          )}
        />
      )}
    </span>
  )
}
