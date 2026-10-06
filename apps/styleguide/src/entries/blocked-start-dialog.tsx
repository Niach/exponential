import blockedStart from "@exp/domain-contract/fixtures/blocked-start.json"
import issueGraphGeometry from "@exp/domain-contract/fixtures/issue-graph-geometry.json"
import {
  IssueChip,
  WaveGraph,
  waveGraphSize,
  type StatusGlyphProps,
  type WaveGraphEdge,
  type WaveGraphNode,
} from "@exp/ui"

import { PromptSpecimen } from "./dialog-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

// EXP-980/SLOP-3: the blocked-start dialog, Cancel · Start anyway · Stacked PR.
//
// The app's dialog (`apps/web/src/components/blocked-start-dialog.tsx`) reads
// the team's synced relations to draw its graph, and no entry here imports an
// app composition, so this is the device-settings precedent: the dialog's
// REAL `@exp/ui` parts (header, title, description, the issue chip, the wave
// grid, footer, buttons) in its order, over static rows. Every word comes
// from the contract fixture `blocked-start.json` (byte-locked ×4), so the
// specimen cannot drift from what the four clients print.

const { copy, notes, planCases } = blockedStart
const geometry = issueGraphGeometry.constants

/** The fields a specimen reads off a `planCases` row. */
interface PlanCase {
  name: string
  subject: { identifier: string }
  line: { identifier: string }[]
  reason: string | null
  note: string | null
  planNote: string | null
}

/** The fixture's case, or a build that fails naming what moved. */
function planCase(pick: (row: PlanCase) => boolean, what: string): PlanCase {
  const found = (planCases as PlanCase[]).find(pick)
  if (!found) throw new Error(`blocked-start.json has no ${what} case`)
  return found
}

// Stacked PR ENABLED over a line of three: the plan note says what starts
// first. Stacked PR DISABLED by a fork: the reason's note takes its place.
const LINE = planCase(
  (row) => row.planNote !== null && row.line.length === 2,
  `two-blocker line`
)
const FORK = planCase((row) => row.reason === `many`, `fork`)

const OPEN_GLYPH: StatusGlyphProps = {
  icon: `circle-dashed`,
  colorClass: `text-muted-foreground`,
}

/** Titles for the fixture's identifiers; the contract names issues only. */
const TITLES: Record<string, string> = {
  [`APP-8`]: `Rotate the signing key`,
  [`APP-9`]: `Session table migration`,
  [`APP-10`]: `Token store`,
  [`APP-11`]: `Session refresh`,
  [`APP-20`]: `Sign-in screen`,
}

function chip(identifier: string, className?: string) {
  return (
    <IssueChip
      identifier={identifier}
      title={TITLES[identifier] ?? identifier}
      status={OPEN_GLYPH}
      className={className}
    />
  )
}

/**
 * THE blocks mini-graph as the dialog draws it (`IssueGraphView` in the
 * app's `components/issue-graph.tsx`): `WaveGraph` turned vertical at the
 * contract's geometry (`issue-graph-geometry.json`), the subject in the
 * accent ring, inside the same scroll box: the grid plus its inset, capped at
 * the contract's viewport and scrolling past it. `rows` are the waves, top
 * (the first blockers) to bottom (the subject); an edge runs from every box
 * of a row to the first box of the row below.
 */
function BlockGraph({ rows }: { rows: string[][] }) {
  const nodes: WaveGraphNode[] = rows.flatMap((row, wave) =>
    row.map((id, lane) => ({ id, wave, lane }))
  )
  const edges: WaveGraphEdge[] = rows.flatMap((row, wave) => {
    const below = rows[wave + 1]?.[0]
    return below === undefined
      ? []
      : row.map((id) => ({ from: id, to: below, style: `plain` as const }))
  })
  const subject = rows.at(-1)?.[0]
  const grid = waveGraphSize(
    nodes,
    {
      nodeWidth: geometry.nodeWidth,
      nodeHeight: geometry.nodeHeight,
      waveGap: geometry.waveGap,
      laneGap: geometry.laneGap,
    },
    `vertical`
  )
  const width = grid.width + 2 * geometry.inset
  const height = grid.height + 2 * geometry.inset
  return (
    <div className="flex flex-col gap-2">
      <div
        className="overflow-auto"
        style={{
          maxWidth: Math.min(width, geometry.maxViewWidth),
          maxHeight: Math.min(height, geometry.maxViewHeight),
        }}
      >
        <div style={{ padding: geometry.inset, width, height }}>
          <WaveGraph
            nodes={nodes}
            edges={edges}
            nodeWidth={geometry.nodeWidth}
            nodeHeight={geometry.nodeHeight}
            waveGap={geometry.waveGap}
            laneGap={geometry.laneGap}
            orientation="vertical"
            edgeStrokeWidth={geometry.edgeStroke}
            renderNode={(id) => chip(id, `w-full`)}
            nodeProps={(id) => ({
              className: id === subject ? `ring-1 ring-primary` : undefined,
            })}
          />
        </div>
      </div>
    </div>
  )
}

function BlockedStartSpecimen({
  caption,
  blocker,
  rows,
  note,
  stackable,
}: {
  caption: string
  /** The subject's direct open blocker, named in the sentence. */
  blocker: string
  rows: string[][]
  /** The plan note, or the reason Stacked PR is disabled. */
  note: string | null
  stackable: boolean
}) {
  const suffix = stackable ? copy.bodySuffixStackable : copy.bodySuffix
  const [, suffixGlue = ``, suffixRest = ``] = /^(\S*)(.*)$/s.exec(suffix) ?? []
  return (
    // The shared prompt card (EXP-1215): the sentence is the body, the graph
    // and its note sit in the content slot, the answers in ONE pill row.
    <PromptSpecimen
      caption={caption}
      title={copy.title}
      body={
        // The chip flows INLINE in the sentence, glued to the suffix's
        // leading "." so a wrap never starts a line with it.
        <div className="leading-6">
          {copy.bodyPrefix}
          <span className="whitespace-nowrap">
            {chip(blocker)}
            {suffixGlue}
          </span>
          {suffixRest}
        </div>
      }
      actions={[
        { label: `Cancel` },
        { label: copy.startAnyway },
        { label: copy.stackedPr, role: `primary`, disabled: !stackable },
      ]}
    >
      <div className="flex flex-col gap-2">
        <BlockGraph rows={rows} />
        {note !== null && (
          <div className="text-xs text-muted-foreground">{note}</div>
        )}
      </div>
    </PromptSpecimen>
  )
}

/** A line's rows: its members bottom first, then the subject. */
function lineRows(row: PlanCase): string[][] {
  return [...row.line.map((member) => [member.identifier]), [row.subject.identifier]]
}

export const entry: StyleguideEntry = {
  id: `blocked-start-dialog`,
  section: `special`,
  owner: `EXP-980`,
  title: `Blocked-start dialog`,
  blurb: `What starting a BLOCKED issue asks first (EXP-980, reworked by SLOP-3). The sentence names the direct open blockers as issue chips flowing INLINE in its text, the last one glued to the suffix's "." so a wrap never opens a line with it (EXP-1167; desktop and iOS print one text run, Android stacks the prefix, the chip row and the suffix without its "." in a column); under it THE blocks mini-graph draws the whole transitive chain, top = the first blockers, bottom = the subject in the accent ring. It is the shared prompt card (EXP-1215): the sentence is its body, the graph its content slot. Three answers in ONE row of 32px pills, in this order: Cancel, "${copy.startAnyway}" (a plain pill), "${copy.stackedPr}" (the primary pill). "${copy.stackedPr}" builds the dependency LINE bottom-up and is never hidden: over a line of two or more issues the plan note under the graph says which one starts first; while it cannot stack (a batch, a cycle, a fork, another repository, more than ${blockedStart.maxRun} issues, a member still running) the button is DISABLED and the reason's note takes the plan note's place, and the sentence drops its stacked half. A batch asks with its own title and one line of body ("${copy.batchTitle}"). The copy, the reasons and their order are the contract's (\`blocked-start.json\`, byte-locked ×4). Phones keep the three answers; iOS hosts the graph in a fitted sheet instead of an alert.`,
  status: {
    web: {
      state: `ok`,
      symbol: `BlockedStartDialog`,
      file: `apps/web/src/components/blocked-start-dialog.tsx`,
      note: `copy and the stack plan are lib/blocked-start.ts; the graph is TeamIssueGraph`,
    },
    desktop: {
      state: `ok`,
      symbol: `ChatScreenView::prompt_blocked_start / render_blocked_panel`,
      file: `apps/desktop/crates/ui/src/chat_screen.rs`,
      note: `a native alert on the page; inside the composer dialog the same question takes over its body`,
    },
    ios: {
      state: `ok`,
      symbol: `BlockedStartSheet`,
      file: `apps/ios/Exponential/UI/Agent/BlockedStartSheet.swift`,
      note: `a fitted sheet rather than an alert, because it hosts the graph`,
    },
    android: {
      state: `ok`,
      symbol: `BlockedStartDialog`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/AgentScreen.kt`,
    },
  },
  island: () => (
    <div className="grid gap-6">
      <BlockedStartSpecimen
        caption="Stacked PR enabled: the plan note says what starts first."
        blocker={LINE.line.at(-1)!.identifier}
        rows={lineRows(LINE)}
        note={LINE.planNote}
        stackable
      />
      <BlockedStartSpecimen
        caption="Stacked PR disabled: the reason takes the plan note's place."
        blocker={FORK.line.at(-1)!.identifier}
        rows={[[`APP-8`, `APP-9`], ...lineRows(FORK)]}
        note={FORK.note ?? notes.many}
        stackable={false}
      />
    </div>
  ),
}
