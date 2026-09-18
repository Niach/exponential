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
