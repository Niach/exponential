import type * as React from "react"
import { AgentRunMark, type RunMarkState } from "./agent-brand-mark"
import { cn } from "./cn"
import { ListRow } from "./glass-rows"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./tooltip"
import { TREE_BASE, TREE_INDENT, type TreeGuide } from "./tree-guides"
import { TreeGuides } from "./tree-guides-view"

// EXP-1248: THE session row, x4 (fixture `list-item.json`). One anatomy in two
// sizes: [tree guides][run mark at 12 + 14·depth][mono id · title][caption,
// big only][device icon]. No fold chevron, no trailing chevron, no buttons:
// any inline control before the text pushes a parent's mark or title off its
// child's, which is the whole bug this row exists to end.

export type SessionRowSize = `small` | `big`

/** The caption's tone, the `session-display.json` statusTone palette. */
export type SessionRowTone = `muted` | `amber` | `emerald` | `sky`

const TONE_CLASS: Record<SessionRowTone, string> = {
  muted: `text-muted-foreground`,
  amber: `text-amber-400`,
  emerald: `text-emerald-400`,
  sky: `text-sky-400`,
}

/** Row heights per size (`list-item.json` geometry small 32 / big 52). */
const SIZE_CLASS: Record<SessionRowSize, string> = {
  small: `h-8`,
  big: `h-[52px]`,
}

export interface SessionRowProps
  extends Omit<React.ComponentProps<`div`>, `title` | `onClick`> {
  size?: SessionRowSize
  /** The run's `coding_sessions.agent`: whose brand mark leads. */
  agent: string | null | undefined
  /** The mark's state; absent = a paused run's bare mark. */
  markState?: RunMarkState
  /** An issue run's identifier or a batch's `EXP-874 +2`; null otherwise. */
  identifier?: string | null
  title: string
  /** The big row's second line (`sessionRowCaption`); ignored when small. */
  caption?: string | null
  captionTone?: SessionRowTone
  /** Nesting depth under a parent run: 14px per level. */
  depth?: number
  /** This row's connector geometry (`treeGuides(depths)[index]`). */
  guide?: TreeGuide | null
  /** The host device's glyph (a concept icon component). */
  deviceIcon?: React.ComponentType<{ className?: string }>
  /** The host device's name: the device icon's tooltip. */
  deviceName?: string | null
  active?: boolean
  /** An offline host: the row dims. */
  dimmed?: boolean
  /** The mark badge's ring = the ground the row sits on. */
  ringClassName?: string
  onClick?: () => void
}

export function SessionRow({
  size = `big`,
  agent,
  markState,
  identifier = null,
  title,
  caption = null,
  captionTone = `muted`,
  depth = 0,
  guide = null,
  deviceIcon: DeviceIcon,
  deviceName = null,
  active = false,
  dimmed = false,
  ringClassName = `ring-background`,
  onClick,
  className,
  style,
  ...props
}: SessionRowProps) {
  const big = size === `big`
  return (
    <ListRow
      interactive
      active={active}
      onClick={onClick}
      data-session-row={size}
      className={cn(
        `relative gap-2 py-0 pr-3`,
        SIZE_CLASS[size],
        dimmed && `opacity-60`,
        className
      )}
      style={{ ...style, paddingLeft: `${TREE_BASE + depth * TREE_INDENT}px` }}
      {...props}
    >
      <TreeGuides guide={guide} />
      <AgentRunMark agent={agent} state={markState} ringClassName={ringClassName} />
      <div data-slot="session-row-text" className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          {identifier && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {identifier}
            </span>
          )}
          <span className="truncate">{title}</span>
        </div>
        {big && caption && (
          <div
            data-slot="session-row-caption"
            className={cn(`truncate text-xs`, TONE_CLASS[captionTone])}
          >
            {caption}
          </div>
        )}
      </div>
      {DeviceIcon && <SessionRowDevice icon={DeviceIcon} name={deviceName} />}
    </ListRow>
  )
}

/** Fixed, never truncated: WHERE the run is, its name on hover. The row
 *  carries its own provider so a list renders outside the app shell too. */
function SessionRowDevice({
  icon: Icon,
  name,
}: {
  icon: React.ComponentType<{ className?: string }>
  name: string | null
}) {
  const glyph = (
    <span
      data-slot="session-row-device"
      className="flex shrink-0 items-center justify-center"
    >
      <Icon className="size-3.5 text-muted-foreground" />
    </span>
  )
  if (!name) return glyph
  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>{glyph}</TooltipTrigger>
        <TooltipContent side="right">{name}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}
