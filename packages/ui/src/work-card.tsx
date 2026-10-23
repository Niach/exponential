import { useState, type ReactNode } from "react"
import type { IconConcept } from "@exp/icons"
import {
  WORK_CARD_GEOMETRY,
  WORK_CARD_SEPARATOR,
  type WorkCard as WorkCardModel,
  type WorkCardLane,
  type WorkCardRow,
  type WorkState,
} from "@exp/domain-contract/work-card"
import fixture from "@exp/domain-contract/fixtures/work-card.json"

import { cn } from "./cn"
import { Spinner } from "./feedback"
import { conceptIcon } from "./icons.generated"

// EXP-1240 (EXP-1259 D12): THE Work card, ONE card for a claude workflow, a
// task list and a subagent, replacing WorkflowCard, TaskListBlock and
// SubagentGroupRow. The model is the contract's (`workCard()` in
// `@exp/domain-contract/work-card`, fixture `work-card.json` ×4); this only
// paints it: a header (kind icon, name, kind word, progress · elapsed), a
// segment bar (phases or tasks), then hairline-divided rows. No report text,
// no token counts, no JSON. A lane row (and a subagent card) opens that
// lane's transcript through the host's callbacks; a task row does nothing.

const TEST_IDS = fixture.testIds
const G = WORK_CARD_GEOMETRY

const DoneIcon = conceptIcon(`ui-check`)
const FailedIcon = conceptIcon(`ui-close`)
const PendingIcon = conceptIcon(`status-backlog`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)

/** Which lane a row's click names: `phase` = the phase's POSITION in the
 *  card's phase order (the same order as `segments`), `lane` = the lane's
 *  position inside that phase. A folded phase counts as its own position. */
export interface WorkCardLaneTarget {
  phase: number
  lane: number
}

/** One folded phase the host spelled out for the folded row's in-place
 *  expansion (the model folds them into one row and drops their lanes). */
export interface WorkCardFoldedPhase {
  title: string
  duration: string | null
  lanes: readonly WorkCardLane[]
}

function StateGlyph({ state }: { state: WorkState }) {
  if (state === `running`) return <Spinner size={12} label="Running" />
  if (state === `done`) {
    return <DoneIcon className="size-3 shrink-0 text-muted-foreground" />
  }
  if (state === `failed`) {
    return <FailedIcon className="size-3 shrink-0 text-rose-400" />
  }
  return <PendingIcon className="size-3 shrink-0 text-muted-foreground/60" />
}

function LaneDot({ state }: { state: string }) {
  return (
    <span
      aria-hidden
      data-state={state}
      className={cn(
        `size-1.5 shrink-0 rounded-full`,
        state === `running`
          ? `bg-emerald-400`
          : state === `error`
            ? `bg-rose-400`
            : state === `queued`
              ? `border border-muted-foreground/60`
              : `bg-muted-foreground/60`
      )}
    />
  )
}

function LaneRow({
  lane,
  onOpen,
}: {
  lane: WorkCardLane
  onOpen?: () => void
}) {
  const content = (
    <>
      <LaneDot state={lane.state} />
      <span className="min-w-0 truncate">{lane.label}</span>
      {lane.model && (
        <span className="shrink-0 text-muted-foreground">{lane.model}</span>
      )}
      <span className="ml-auto flex shrink-0 items-center tabular-nums text-muted-foreground">
        {lane.state === `running` ? (
          <Spinner size={12} label="Running" />
        ) : (
          lane.duration
        )}
      </span>
    </>
  )
  const className = cn(
    `flex w-full min-w-0 items-center gap-2 pr-3 text-left`,
    lane.state === `error` && `text-rose-400`
  )
  const style = { height: G.lane, paddingLeft: G.laneIndent }
  if (!onOpen) {
    return (
      <div className={className} style={style} data-testid={TEST_IDS.lane}>
        {content}
      </div>
    )
  }
  return (
    <button
      type="button"
      className={cn(className, `hover:bg-glass-active/50`)}
      style={style}
      data-testid={TEST_IDS.lane}
      onClick={onOpen}
    >
      {content}
    </button>
  )
}

function Trailing({ children }: { children: ReactNode }) {
  return (
    <span className="ml-auto flex shrink-0 items-center gap-1.5 tabular-nums text-muted-foreground">
      {children}
    </span>
  )
}

function FoldedRow({
  row,
  phases,
  onOpenLane,
}: {
  row: Extract<WorkCardRow, { type: `folded` }>
  phases?: readonly WorkCardFoldedPhase[]
  onOpenLane?: (target: WorkCardLaneTarget) => void
}) {
  const [open, setOpen] = useState(false)
  const expandable = phases !== undefined && phases.length > 0
  const head = (
    <>
      <DoneIcon className="size-3 shrink-0 text-muted-foreground" />
      <span className="min-w-0 truncate">{row.label}</span>
      <span className="shrink-0 text-muted-foreground">{row.lanes}</span>
      <Trailing>
        {row.duration}
        {expandable && (
          <ChevronIcon
            className={cn(
              `size-3.5 transition-transform duration-fast ease-standard motion-reduce:transition-none`,
              open && `rotate-90`
            )}
          />
        )}
      </Trailing>
    </>
  )
  return (
    <div data-testid={TEST_IDS.folded}>
      {expandable ? (
        <button
          type="button"
          aria-expanded={open}
          className="flex w-full min-w-0 items-center gap-2 px-3 text-left hover:bg-glass-active/50"
          style={{ height: G.row }}
          onClick={() => setOpen((value) => !value)}
        >
          {head}
        </button>
      ) : (
        <div
          className="flex min-w-0 items-center gap-2 px-3"
          style={{ height: G.row }}
        >
          {head}
        </div>
      )}
      {open &&
        phases?.map((phase, phaseIndex) => (
          <div key={`${phase.title}-${phaseIndex}`}>
            <div
              className="flex min-w-0 items-center gap-2 pr-3 text-muted-foreground"
              style={{ height: G.lane, paddingLeft: G.laneIndent }}
            >
              <span className="min-w-0 truncate">{phase.title}</span>
              <Trailing>{phase.duration}</Trailing>
            </div>
            {phase.lanes.map((lane, laneIndex) => (
              <LaneRow
                key={laneIndex}
                lane={lane}
                onOpen={
                  onOpenLane
                    ? () => onOpenLane({ phase: phaseIndex, lane: laneIndex })
                    : undefined
                }
              />
            ))}
          </div>
        ))}
    </div>
  )
}

function PhaseRow({
  row,
  phase,
  onOpenLane,
}: {
  row: Extract<WorkCardRow, { type: `phase` }>
  phase: number
  onOpenLane?: (target: WorkCardLaneTarget) => void
}) {
  return (
    <div data-state={row.state}>
      <div
        className={cn(
          `flex min-w-0 items-center gap-2 px-3`,
          row.state === `pending` && `text-muted-foreground`
        )}
        style={{ height: G.row }}
      >
        <StateGlyph state={row.state} />
        <span className="min-w-0 truncate">{row.title}</span>
        {row.detail && (
          <span className="min-w-0 truncate text-muted-foreground">
            {row.detail}
          </span>
        )}
        {row.duration && <Trailing>{row.duration}</Trailing>}
      </div>
      {row.lanes.map((lane, laneIndex) => (
        <LaneRow
          key={laneIndex}
          lane={lane}
          onOpen={
            onOpenLane ? () => onOpenLane({ phase, lane: laneIndex }) : undefined
          }
        />
      ))}
    </div>
  )
}

/** The phase POSITION each row stands for: a leading folded row covers the
 *  first `segments - (rows - 1)` phases, every other row is one phase. */
function rowPhases(card: WorkCardModel): number[] {
  const folded = card.rows[0]?.type === `folded`
  const foldedCount = folded ? card.segments.length - (card.rows.length - 1) : 0
  return card.rows.map((_, index) =>
    folded ? (index === 0 ? 0 : foldedCount + index - 1) : index
  )
}

/** The Work card's HEADER row: kind icon, name, the muted kind word, then
 *  trailing (a 12px spinner while `running`, then the muted text). Shared by
 *  the Work card and THE session card (EXP-1291 `session-card.json`). */
export function WorkCardHeader({
  icon,
  name,
  kindWord,
  kindWordClassName,
  running,
  trailing,
  after,
}: {
  icon: IconConcept
  name: string
  kindWord: string | null
  /** Tone override for the kind word (the session card's amber/rose). */
  kindWordClassName?: string
  running: boolean
  trailing: string | null
  /** A glyph after the trailing text (the session card's chevron). */
  after?: ReactNode
}) {
  const Icon = conceptIcon(icon)
  return (
    <div className="flex min-w-0 items-center gap-2 px-3" style={{ height: G.header }}>
      <Icon className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="min-w-0 truncate font-medium">{name}</span>
      {kindWord && (
        <span className={cn(`shrink-0 text-muted-foreground`, kindWordClassName)}>
          {kindWord}
        </span>
      )}
      {(running || trailing || after) && (
        <Trailing>
          {running && <Spinner size={12} label="Running" />}
          {trailing}
          {after}
        </Trailing>
      )}
    </div>
  )
}

/** `sessionCard()` from `@exp/domain-contract/session-tree`, structurally. */
export interface SessionCardModel {
  sessionId: string
  icon: string
  name: string
  kindWord: string
  state: string
  tone: string
  elapsed: string | null
  running: boolean
  /** EXP-1323: the live status line, the card's muted second line. */
  caption?: string | null
  testId: string
}

export const SESSION_CARD_TONE: Record<string, string> = {
  amber: `text-amber-400`,
  rose: `text-rose-400`,
  foreground: `text-foreground`,
  muted: ``,
}

/**
 * EXP-1291 THE session card ×4: a session inside a thread (or a thread an
 * agent started) is ONE card = the Work card's header row in the Work
 * card's shell (radius lg, card hairline, no shadow). Tapping it opens that
 * session's raw view. No device, account or branch; EXP-1323: the live
 * `caption` (what it does right now) is the muted second line under the
 * name, aligned with it.
 */
export function SessionCard({
  card,
  onOpen,
  className,
}: {
  card: SessionCardModel
  onOpen?: () => void
  className?: string
}) {
  const header = (
    <WorkCardHeader
      icon={card.icon as IconConcept}
      name={card.name}
      kindWord={card.kindWord}
      kindWordClassName={SESSION_CARD_TONE[card.tone]}
      running={card.running}
      trailing={card.elapsed}
      after={onOpen ? <ChevronIcon className="size-3.5 shrink-0" /> : undefined}
    />
  )
  const caption = card.caption ? (
    <div
      data-slot="session-card-caption"
      className="-mt-1.5 truncate pr-3 pb-2 text-muted-foreground"
      // The name's x: the row's 12px padding + the 14px icon + its 8px gap.
      style={{ paddingLeft: 34 }}
    >
      {card.caption}
    </div>
  ) : null
  const shell = cn(
    `block w-full min-w-0 overflow-clip rounded-lg border border-glass-stroke-card bg-glass-card-opaque text-left text-xs`,
    className
  )
  if (!onOpen) {
    return (
      <div data-testid={card.testId} data-state={card.state} className={shell}>
        {header}
        {caption}
      </div>
    )
  }
  return (
    <button
      type="button"
      data-testid={card.testId}
      data-state={card.state}
      className={cn(shell, `hover:bg-glass-active/50`)}
      onClick={onOpen}
    >
      {header}
      {caption}
    </button>
  )
}

export function WorkCard({
  card,
  foldedPhases,
  onOpenLane,
  onOpenSubagent,
  children,
  className,
}: {
  /** `workCard(input)` from `@exp/domain-contract/work-card`. */
  card: WorkCardModel
  /** The phases behind a folded row, for its in-place expansion; absent =
   *  the folded row carries no chevron. */
  foldedPhases?: readonly WorkCardFoldedPhase[]
  /** A workflow lane row was picked: open that lane's transcript. */
  onOpenLane?: (target: WorkCardLaneTarget) => void
  /** A subagent card was picked: open its transcript. */
  onOpenSubagent?: () => void
  /** Rows the host appends under the card's own (an amber duplicate
   *  warning); never report text. */
  children?: ReactNode
  className?: string
}) {
  const trailing = [card.progress, card.elapsed]
    .filter((part): part is string => part !== null)
    .join(WORK_CARD_SEPARATOR)
  const phases = card.kind === `workflow` ? rowPhases(card) : []
  const subagentOpens = card.kind === `subagent` && onOpenSubagent !== undefined

  const header = (
    <WorkCardHeader
      icon={card.icon as IconConcept}
      name={card.name}
      kindWord={card.kindWord !== card.name ? card.kindWord : null}
      running={card.running}
      trailing={trailing || null}
    />
  )

  return (
    <div
      data-testid={TEST_IDS.card}
      data-kind={card.kind}
      className={cn(
        `min-w-0 overflow-clip rounded-lg border border-glass-stroke-card bg-glass-card-opaque text-xs`,
        className
      )}
    >
      {subagentOpens ? (
        <button
          type="button"
          className="block w-full min-w-0 text-left hover:bg-glass-active/50"
          onClick={onOpenSubagent}
        >
          {header}
          <SubagentRows rows={card.rows} />
        </button>
      ) : (
        header
      )}
      {card.segments.length > 0 && (
        <div
          aria-hidden
          data-slot="work-card-segments"
          className="flex px-3 pb-2.5"
          style={{ gap: G.segmentGap }}
        >
          {card.segments.map((segment, index) => (
            <span
              key={index}
              data-state={segment.state}
              className="relative flex-1 overflow-hidden rounded-full bg-muted"
              style={{ height: G.segmentHeight }}
            >
              <span
                className={cn(
                  `absolute inset-y-0 left-0 rounded-full`,
                  segment.state === `failed` ? `bg-rose-400` : `bg-foreground`
                )}
                style={{ width: `${segment.fill * 100}%` }}
              />
            </span>
          ))}
        </div>
      )}
      {!subagentOpens && card.rows.length > 0 && (
        <div className="divide-y divide-glass-stroke border-t border-glass-stroke">
          {card.rows.map((row, index) => {
            if (row.type === `folded`) {
              return (
                <FoldedRow
                  key={`folded`}
                  row={row}
                  phases={foldedPhases}
                  onOpenLane={onOpenLane}
                />
              )
            }
            if (row.type === `phase`) {
              return (
                <PhaseRow
                  key={`phase-${phases[index]}`}
                  row={row}
                  phase={phases[index]}
                  onOpenLane={onOpenLane}
                />
              )
            }
            if (row.type === `task`) {
              return (
                <div
                  key={`task-${index}`}
                  className="flex min-w-0 items-center gap-2 px-3"
                  style={{ height: G.row }}
                >
                  <StateGlyph state={row.state} />
                  <span className="min-w-0 truncate">{row.label}</span>
                </div>
              )
            }
            return (
              <div
                key={`status-${index}`}
                className="flex min-w-0 items-center px-3 text-muted-foreground"
                style={{ height: G.row }}
              >
                <span className="min-w-0 truncate">{row.label}</span>
              </div>
            )
          })}
        </div>
      )}
      {children}
    </div>
  )
}

/** A subagent card's rows inside its one button (the whole card opens). */
function SubagentRows({ rows }: { rows: readonly WorkCardRow[] }) {
  return (
    <>
      {rows.map((row, index) =>
        row.type === `status` ? (
          <div
            key={index}
            className="flex min-w-0 items-center px-3 text-muted-foreground"
            style={{ height: G.row }}
          >
            <span className="min-w-0 truncate">{row.label}</span>
          </div>
        ) : null
      )}
    </>
  )
}
