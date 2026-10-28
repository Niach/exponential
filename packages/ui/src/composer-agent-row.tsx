import { BoardGlyph, conceptIcon, ExponentialLogo } from "@exp/ui"
import { cn } from "@/lib/utils"
import {
  COMPOSER_AGENT_ROW,
  TEMPORARY_PICK,
  type ComposerAgentRowState,
} from "@exp/domain-contract/composer-agent-row"
import { isAssistant } from "@exp/domain-contract/agent-settings"

// S-3 (composer-home.json `agentRow`, styleguide `composer-agent-row`): THE
// agent row over the New-chat composer card. The label line (`To <name>` /
// `Temporary session`), then single-select circles: the chats agent BUBBLE
// (`AgentBubbleButton`'s disc on bg muted inside its background gap) at
// `size`, no name under it (the name is the label and the tooltip): the
// viewer's Assistant first (THE brand mark in the foreground colour), the
// team agents (icon in its colour), then ONE dotted circle = Temporary
// session. The selected circle wears the foreground ring, the others the
// glass hairline. Exponential-specific: not an Exponential UI primitive.

const TemporaryIcon = conceptIcon(COMPOSER_AGENT_ROW.temporaryIcon as `ui-icon-placeholder`)

const { size, ringWidth, ringGap, gap, labelToRow, testIds } = COMPOSER_AGENT_ROW

/** One circle's agent: the synced definition the row lists. */
export interface ComposerAgentRowAgent {
  id: string
  name: string
  icon: string | null
  color: string | null
  sortOrder: number
  role?: string | null
  ownerId: string
}

export function composerAgentOptionTestId(id: string): string {
  return testIds.option.replace(`{id}`, id)
}

export function ComposerAgentRow<T extends ComposerAgentRowAgent>({
  row,
  onPick,
  disabled,
  className,
}: {
  row: ComposerAgentRowState<T>
  onPick: (id: string) => void
  disabled?: boolean
  className?: string
}) {
  return (
    <div data-testid={testIds.row} className={cn(`flex min-w-0 flex-col`, className)} style={{ gap: labelToRow }}>
      <span data-testid={testIds.label} className="px-1 text-xs text-muted-foreground">
        {row.label}
      </span>
      <div
        role="radiogroup"
        aria-label={row.label}
        className="flex min-w-0 items-center overflow-x-auto px-1 py-[3px] [scrollbar-width:none]"
        style={{ gap }}
      >
        {row.agents.map((agent) => (
          <AgentCircle
            key={agent.id}
            id={agent.id}
            name={agent.name}
            selected={row.selected === agent.id}
            disabled={disabled}
            onPick={onPick}
          >
            {isAssistant(agent) ? (
              <ExponentialLogo variant="light" size={size / 2} className="text-foreground" />
            ) : (
              <BoardGlyph
                board={{ icon: agent.icon ?? undefined, color: agent.color ?? undefined }}
                className="size-5"
              />
            )}
          </AgentCircle>
        ))}
        <AgentCircle
          id={TEMPORARY_PICK}
          name={COMPOSER_AGENT_ROW.copy.temporary}
          selected={row.selected === TEMPORARY_PICK}
          disabled={disabled}
          onPick={onPick}
        >
          <TemporaryIcon className="size-5 text-muted-foreground" />
        </AgentCircle>
      </div>
    </div>
  )
}

/** One circle: the chats bubble's disc at the row's size. */
export function AgentCircle({
  id,
  name,
  selected,
  disabled,
  onPick,
  children,
}: {
  id: string
  name: string
  selected: boolean
  disabled?: boolean
  onPick: (id: string) => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      aria-label={name}
      title={name}
      disabled={disabled}
      data-testid={composerAgentOptionTestId(id)}
      data-selected={selected || undefined}
      // Tapping the selected circle keeps it: the row is never empty.
      onClick={() => onPick(id)}
      className={cn(
        `relative flex shrink-0 rounded-full outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:opacity-50`,
        selected ? `border-foreground` : `border-transparent`
      )}
      style={{ width: size, height: size, borderWidth: ringWidth, padding: ringGap }}
    >
      {!selected && (
        <span aria-hidden className="pointer-events-none absolute -inset-px rounded-full border border-glass-stroke" />
      )}
      <span className="flex size-full items-center justify-center rounded-full bg-muted">{children}</span>
    </button>
  )
}
