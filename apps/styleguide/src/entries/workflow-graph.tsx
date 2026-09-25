import {
  BUILTIN_STATUS_COLOR_CLASS,
  Button,
  categoryStatusIcon,
  conceptIcon,
  LiveDot,
  WorkflowGraphView,
  type WaveGraphEdge,
  type WorkflowGraphNode,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1033 — the workflow screen's graph, as a real island.
//
// A node is the app's ISSUE CHIP on the wave grid (wave = column, lane = row),
// with its ONE caption trailing INSIDE the chip: no circles, no second line, no
// "Leaf" / "Contract · high risk" sub-subtitle. A compound node (a batch) rides
// `IssueChipStack`. The whole picture scales down to the container rather than
// growing an inner scrollbar, and the final pull request closes it as one more
// chip after the last wave.

const MergedGlyph = conceptIcon(`notification-pr-merged`)
const ReviewGlyph = conceptIcon(`nav-reviews`)

const landed = <MergedGlyph className="size-3.5 shrink-0" />

/** What a node with nothing happening to it yet falls back to: the issue's
 *  own resolved status, exactly as `IssueChip` draws it. */
const backlogStatus = {
  icon: categoryStatusIcon(`backlog`, 0, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.backlog,
}
const startedStatus = {
  icon: categoryStatusIcon(`started`, 0, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.in_progress,
}

/** Contract → three leaves (one batched, one live, one landed) → integration. */
const NODES: WorkflowGraphNode[] = [
  {
    id: `contract`,
    wave: 0,
    lane: 1,
    title: `EXP-101`,
    name: `Shape of the sync contract`,
    caption: `Landed`,
    tone: `success`,
    glyph: landed,
  },
  {
    id: `batch`,
    wave: 1,
    lane: 0,
    title: `EXP-102 +2`,
    name: `Web, desktop and iOS pickers`,
    caption: `In review`,
    tone: `active`,
    glyph: <ReviewGlyph className="size-3.5 shrink-0" />,
    stacked: true,
  },
  {
    id: `live`,
    wave: 1,
    lane: 1,
    title: `EXP-105`,
    name: `Relay the new wire kinds`,
    caption: `Running`,
    tone: `success`,
    glyph: <LiveDot tone="live" ping className="shrink-0" />,
    running: true,
    selected: true,
  },
  {
    id: `done`,
    wave: 1,
    lane: 2,
    title: `EXP-106`,
    name: `Android list rows`,
    caption: `Landed`,
    tone: `success`,
    glyph: landed,
  },
  {
    id: `blocked`,
    wave: 2,
    lane: 0,
    title: `EXP-108`,
    name: `Document the new wire kinds`,
    caption: `Blocked`,
    tone: `muted`,
    // Nothing is happening to it: the chip reads as the ISSUE does elsewhere.
    status: backlogStatus,
  },
  {
    id: `integration`,
    wave: 2,
    lane: 1,
    title: `EXP-110`,
    name: `Fold the four clients together`,
    caption: `Ready`,
    tone: `muted`,
    status: startedStatus,
  },
]

const EDGES: WaveGraphEdge[] = [
  { from: `contract`, to: `batch`, style: `landed` },
  { from: `contract`, to: `live`, style: `landed` },
  { from: `contract`, to: `done`, style: `landed` },
  { from: `batch`, to: `integration`, style: `plain` },
  { from: `live`, to: `integration`, style: `speculative` },
  { from: `done`, to: `integration`, style: `landed` },
  { from: `batch`, to: `blocked`, style: `plain` },
]

export const entry: StyleguideEntry = {
  id: `workflow-graph`,
  section: `views`,
  owner: `EXP-1014`,
  title: `Workflow graph`,
  blurb: `The workflow screen: issue chips by wave and lane, batched ones stacked, the final pull request closing the run. No settings panel.`,
  status: {
    web: {
      state: `ok`,
      symbol: `WorkflowGraphView`,
      file: `packages/ui/src/workflow-graph.tsx`,
      note: `EXP-1069: the web page mounts the node strip in components/workflow-detail.tsx instead.`,
    },
    desktop: {
      state: `ok`,
      symbol: `workflow_view`,
      file: `apps/desktop/crates/ui/src/workflow_view.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `WorkflowDetailView`,
      file: `apps/ios/Exponential/UI/Workflows/WorkflowDetailView.swift`,
    },
    android: {
      state: `ok`,
      symbol: `WorkflowDetailScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/workflows/WorkflowDetailScreen.kt`,
    },
  },
  island: () => (
    <WorkflowGraphView
      nodes={NODES}
      edges={EDGES}
      finalPr={{
        caption: `#128 · Open`,
        url: `https://github.com/niach/exponential/pull/128`,
        trailing: (
          <Button size="inline" variant="text" className="shrink-0 pl-1 font-medium">
            Merge
          </Button>
        ),
      }}
    />
  ),
}
