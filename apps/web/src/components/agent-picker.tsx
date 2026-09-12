import type { ReactNode } from "react"
import { contract } from "@exp/domain-contract"

import { ClaudeIcon, CodexIcon } from "@/components/icons/brand-icons"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Tabs,
  TabsList,
  TabsTrigger,
  SEGMENTED_ROW,
  SEGMENTED_TAB,
} from "@/components/ui/tabs"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { conceptIcon } from "@/lib/icons.generated"
import { cn } from "@/lib/utils"

// EXP-862: ONE agent picker per platform (web here, desktop
// `coding_selects::agent_picker`, iOS `AgentPickerMenu`, Android
// `AgentPickerPill`). Every surface that switches the coding agent — the
// composer's options row, the device-settings "Default agent" row, the
// launch-options pane's tab strip — renders THIS component, so the trigger
// can never drift into a per-surface copy again.
//
// The trigger is ICON-ONLY: the agent's brand mark plus a chevron. The text
// "Claude Code" / "Codex" appears only in the menu rows and in the tooltip,
// which doubles as the trigger's accessible name.

/** Display labels for the contract's agent ids. Keyed by
 * `contract.codingAgent.values`; `agentLabel` falls back to the raw id, so a
 * future contract value can never render unlabeled. */
export const AGENT_LABELS: Record<string, string> = {
  claude: `Claude Code`,
  codex: `Codex`,
}

/** The label for an agent id — its display name, or the id itself for an id
 * this build does not know (an external ACP binary, a retired agent). */
export function agentLabel(agent: string): string {
  return AGENT_LABELS[agent] ?? agent
}

// Each shipped agent's hand-drawn brand mark. Brand marks are deliberately
// NOT part of the Lucide concept registry (they are not glyphs we may
// restyle); every other glyph on this surface is a concept.
const AGENT_ICONS: Record<
  string,
  (props: React.SVGProps<SVGSVGElement>) => React.JSX.Element
> = {
  claude: ClaudeIcon,
  codex: CodexIcon,
}

// EXP-849: an id outside the shipped set — a retired one (the historical
// `pi`), a future agent, an external ACP binary — has no mark. It gets the
// NEUTRAL agent concept rather than an empty trigger or, worse, claude's
// mark: the same fallback glyph the other three clients draw.
const AgentFallbackIcon = conceptIcon(`settings-agents`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const CheckIcon = conceptIcon(`ui-check`)

/** The glyph for an agent id — its brand mark, or the neutral agent concept
 *  for an id this build ships no mark for. */
export function agentMarkIcon(
  agent: string
): React.ComponentType<{ className?: string }> {
  return AGENT_ICONS[agent] ?? AgentFallbackIcon
}

/** The brand mark itself, sized by the caller. */
export function AgentMark({
  agent,
  className,
}: {
  agent: string
  className?: string
}) {
  const Mark = agentMarkIcon(agent)
  return <Mark className={cn(`size-4 shrink-0`, className)} />
}

/** The menu rows — brand mark + label, a check on the current pick. Exported
 * on its own so a surface that already owns a dropdown (a row's "…" menu)
 * lists the agents without nesting a second trigger inside it. */
export function AgentMenuItems({
  agents,
  value,
  onChange,
}: {
  agents: string[]
  value: string
  onChange: (agent: string) => void
}): ReactNode {
  return agents.map((agent) => (
    <DropdownMenuItem key={agent} onSelect={() => onChange(agent)}>
      <AgentMark agent={agent} />
      <span className="flex-1">{agentLabel(agent)}</span>
      {agent === value && (
        <CheckIcon className="size-4 shrink-0 text-muted-foreground" />
      )}
    </DropdownMenuItem>
  ))
}

/** THE agent picker: brand mark + chevron, the label only in the tooltip. */
export function AgentPicker({
  value,
  onChange,
  agents,
  disabled = false,
  size = `default`,
  align = `start`,
  className,
}: {
  value: string
  onChange: (agent: string) => void
  /** The runnable agent ids of the machine this run lands on. */
  agents: string[]
  disabled?: boolean
  /** `sm` is the muted composer line, `default` every 32px row action. */
  size?: `sm` | `default`
  align?: `start` | `center` | `end`
  className?: string
}) {
  const label = agentLabel(value)
  const small = size === `sm`
  return (
    // Self-contained provider: the picker lands inside dialogs and portals
    // that do not always sit under the app shell's provider, and a tooltip
    // that throws is worse than one that nests.
    <TooltipProvider delayDuration={0}>
      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size={small ? `icon-xs` : `icon-sm`}
                disabled={disabled || agents.length === 0}
                aria-label={label}
                data-slot="agent-picker"
                className={cn(
                  `w-auto gap-0.5 px-1.5 text-foreground/70 hover:text-foreground`,
                  className
                )}
              >
                <AgentMark
                  agent={value}
                  className={small ? `size-3.5` : `size-4`}
                />
                <ChevronDownIcon
                  className={small ? `size-2.5` : `size-3`}
                />
              </Button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent>{label}</TooltipContent>
        </Tooltip>
        <DropdownMenuContent align={align}>
          <AgentMenuItems agents={agents} value={value} onChange={onChange} />
        </DropdownMenuContent>
      </DropdownMenu>
    </TooltipProvider>
  )
}

/** The same picker as a segmented STRIP — the launch-options pane, where the
 * agents sit side by side rather than behind a chevron. Same marks, same
 * labels, the app's one segmented control (`SEGMENTED_ROW`/`SEGMENTED_TAB`). */
export function AgentPickerTabs({
  value,
  onChange,
  agents,
  disabled = false,
  className,
}: {
  value: string
  onChange: (agent: string) => void
  agents: string[]
  disabled?: boolean
  className?: string
}) {
  if (agents.length === 0) return null
  return (
    <div className={cn(SEGMENTED_ROW, className)} data-slot="agent-picker-tabs">
      <Tabs value={value} onValueChange={onChange} className="w-full">
        <TabsList className="w-full">
          {agents.map((agent) => (
            <TabsTrigger
              key={agent}
              value={agent}
              disabled={disabled}
              className={SEGMENTED_TAB}
            >
              <AgentMark agent={agent} className="size-3.5" />
              {agentLabel(agent)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </div>
  )
}

/** Every agent the contract ships — the default list for a surface with no
 * device to ask (the automations editor's "device default" arm). */
export const CONTRACT_AGENTS: string[] = [...contract.codingAgent.values]
