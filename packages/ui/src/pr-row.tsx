import type * as React from "react"
import { useState } from "react"
import { contract } from "@exp/domain-contract"
import { Button } from "./button"
import { cn } from "./cn"
import { ListRow } from "./glass-rows"
import {
  TREE_BASE,
  TREE_INDENT,
  treeGuideCentre,
  treeGuides,
  type TreeGuide,
} from "./tree-guides"
import { TreeGuides } from "./tree-guides-view"

// EXP-1248: THE pull-request row, x4 (fixture `list-item.json`): ONE line,
// [ring lead · mono identifier · title · quiet word]. No branch line, no PR
// number, no counts, no age, no inline Merge. Reviews, PR trees and the
// Guide's stack card all draw it. Two shapes: a TREE nests with tree guides
// (`PrList`), a linear STACK never nests, it hangs off one rail down to its
// base branch (`StackRail`).

/** The ring lead: an open PR, the current stack member, the base branch. */
export type PrNodeState = `open` | `current` | `base`

/** The lead's ring diameter (`list-item.json` geometry prNode). */
const PR_NODE = 12

export function PrNode({
  state,
  className,
}: {
  state: PrNodeState
  className?: string
}) {
  return (
    // A 14px box like the run mark, so tree guides land on its centre.
    <span
      data-slot="pr-node"
      data-state={state}
      className={cn(`relative flex size-3.5 shrink-0 items-center justify-center`, className)}
    >
      <span
        className={cn(
          `flex size-3 items-center justify-center rounded-full border-[1.5px]`,
          state === `base` ? `border-muted-foreground/60` : `border-emerald-500`
        )}
      >
        {state === `current` && (
          <span data-slot="pr-node-fill" className="size-1.5 rounded-full bg-emerald-500" />
        )}
      </span>
    </span>
  )
}

/** Which rail segments a stack row draws: into the node from above, out of
 *  it below. They stop at the ring, never cross it. */
export interface PrRailSegments {
  above?: boolean
  below?: boolean
}

export interface PrRowProps
  extends Omit<React.ComponentProps<`div`>, `title` | `onClick`> {
  node?: PrNodeState
  identifier?: string | null
  /** The PR's issue title, or the base branch's name on a `base` row. */
  title: string
  /** A quiet trailing word (`stack` on a stack's top row). */
  word?: string | null
  depth?: number
  /** Tree shape: this row's connector geometry. */
  guide?: TreeGuide | null
  /** Stack shape: the rail through the node centres. */
  rail?: PrRailSegments | null
  active?: boolean
  onClick?: () => void
  /** Trailing content after the word (an external-link glyph, a ghost). */
  trailing?: React.ReactNode
}

export function PrRow({
  node = `open`,
  identifier = null,
  title,
  word = null,
  depth = 0,
  guide = null,
  rail = null,
  active = false,
  onClick,
  trailing,
  className,
  style,
  ...props
}: PrRowProps) {
  const base = node === `base`
  const centre = treeGuideCentre(depth)
  const half = PR_NODE / 2
  return (
    <ListRow
      interactive={onClick != null}
      active={active}
      onClick={onClick}
      data-pr-row={node}
      className={cn(`relative h-10 gap-2 py-0 pr-3 md:h-9`, className)}
      style={{ ...style, paddingLeft: `${TREE_BASE + depth * TREE_INDENT}px` }}
      {...props}
    >
      <TreeGuides guide={guide} />
      {rail?.above && (
        <span
          aria-hidden
          data-slot="pr-rail-above"
          className="pointer-events-none absolute top-0 w-px bg-glass-stroke-strong"
          style={{ left: centre - 0.5, height: `calc(50% - ${half}px)` }}
        />
      )}
      {rail?.below && (
        <span
          aria-hidden
          data-slot="pr-rail-below"
          className="pointer-events-none absolute bottom-0 w-px bg-glass-stroke-strong"
          style={{ left: centre - 0.5, top: `calc(50% + ${half}px)` }}
        />
      )}
      <PrNode state={node} />
      {identifier && (
        <span className="min-w-[4.5rem] shrink-0 font-mono text-xs text-muted-foreground">
          {identifier}
        </span>
      )}
      <span
        className={cn(
          `min-w-0 flex-1 truncate`,
          base ? `font-mono text-xs text-muted-foreground` : `text-sm`
        )}
      >
        {title}
      </span>
      {word && (
        <span data-slot="pr-row-word" className="shrink-0 text-[11.5px] text-muted-foreground">
          {word}
        </span>
      )}
      {trailing}
    </ListRow>
  )
}

/** One row of a PR tree: `depth` nests it under its parent. */
export interface PrListRow {
  key: string
  identifier?: string | null
  title: string
  depth?: number
  node?: PrNodeState
  word?: string | null
  active?: boolean
  onOpen?: () => void
  trailing?: React.ReactNode
}

/** A PR TREE (or a flat run of single PRs): rows nest with tree guides. */
export function PrList({
  rows,
  className,
}: {
  rows: readonly PrListRow[]
  className?: string
}) {
  const guides = treeGuides(rows.map((row) => row.depth ?? 0))
  return (
    // Gapless: nothing for a connector to bridge.
    <div data-slot="pr-list" className={cn(`flex flex-col`, className)}>
      {rows.map((row, index) => (
        <PrRow
          key={row.key}
          node={row.node}
          identifier={row.identifier}
          title={row.title}
          word={row.word}
          depth={row.depth ?? 0}
          guide={guides[index]}
          active={row.active}
          onClick={row.onOpen}
          trailing={row.trailing}
        />
      ))}
    </div>
  )
}

/** One member of a linear stack. */
export interface StackRailMember {
  key: string
  identifier: string
  title: string
  /** The member the page is showing: the filled node + the active wash. */
  current?: boolean
}

/** A STACK, top-first, on one rail down to its base-branch row. Hovering a
 *  member offers a ghost "Merge through here" when `onMergeThrough` is set
 *  (phones reach it from the row's long-press menu instead). */
export function StackRail<M extends StackRailMember>({
  members,
  baseBranch,
  word = null,
  onOpen,
  onMergeThrough,
  mergeThroughLabel = contract.diffUi.mergeThrough,
  defaultHoveredKey = null,
  className,
}: {
  /** Top member first. */
  members: readonly M[]
  baseBranch: string
  /** The quiet word on the top row (`stack` in Reviews; none on the Guide). */
  word?: string | null
  onOpen?: (member: M) => void
  onMergeThrough?: (member: M) => void
  mergeThroughLabel?: string
  /** The styleguide pins one row's hover state. */
  defaultHoveredKey?: string | null
  className?: string
}) {
  const [hovered, setHovered] = useState<string | null>(defaultHoveredKey)
  const leave = (key: string) =>
    setHovered((current) => (current === key ? null : current))
  return (
    <div data-slot="stack-rail" className={cn(`flex flex-col`, className)}>
      {members.map((member, index) => (
        <PrRow
          key={member.key}
          node={member.current ? `current` : `open`}
          identifier={member.identifier}
          title={member.title}
          word={index === 0 ? word : null}
          rail={{ above: index > 0, below: true }}
          active={member.current}
          onClick={onOpen ? () => onOpen(member) : undefined}
          onMouseEnter={() => setHovered(member.key)}
          onMouseLeave={() => leave(member.key)}
          onFocus={() => setHovered(member.key)}
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
              leave(member.key)
            }
          }}
          trailing={
            onMergeThrough && hovered === member.key ? (
              <Button
                variant="ghost"
                size="xs"
                data-slot="stack-merge-through"
                className="shrink-0 text-muted-foreground"
                onClick={(event) => {
                  event.stopPropagation()
                  onMergeThrough(member)
                }}
              >
                {mergeThroughLabel}
              </Button>
            ) : null
          }
        />
      ))}
      <PrRow node="base" title={baseBranch} rail={{ above: members.length > 0 }} />
    </div>
  )
}
