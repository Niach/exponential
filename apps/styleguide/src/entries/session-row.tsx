import {
  getDeviceIcon,
  type RunMarkState,
  SessionRow,
  type SessionRowSize,
  type SessionRowTone,
  treeGuides,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1248: THE session row, the REAL `@exp/ui` SessionRow in both sizes over
// the same runs: small on the sidebar ground (the main menu's Running), big on
// the page ground (Recent, the phone Agent page, an action's Runs). The
// captions are literal here; the app words them with `sessionRowCaption`
// (fixture `list-item.json`).

const LAPTOP = getDeviceIcon({ kind: `desktop` })
const SERVER = getDeviceIcon({ kind: `server` })

interface SpecimenRun {
  depth: number
  state: RunMarkState
  agent: string
  identifier: string | null
  title: string
  caption: string
  tone: SessionRowTone
  device: `macbook` | `mint`
}

const RUNS: readonly SpecimenRun[] = [
  // A standalone live root, a parent with the two runs it started, an ended chat.
  {
    depth: 0,
    state: `working`,
    agent: `claude`,
    identifier: `EXP-1239`,
    title: `ios double button`,
    caption: `Building · macbook · 21 h`,
    tone: `muted`,
    device: `macbook`,
  },
  {
    depth: 0,
    state: `working`,
    agent: `claude`,
    identifier: null,
    title: `Exponential UI completeness pass`,
    caption: `Building · mint · 2 h`,
    tone: `muted`,
    device: `mint`,
  },
  {
    depth: 1,
    state: `needs_input`,
    agent: `claude`,
    identifier: `VAPP-99`,
    title: `Exponential UI round 2: one stack, ~100% parity`,
    caption: `Needs input · mint · 5 min`,
    tone: `amber`,
    device: `mint`,
  },
  {
    depth: 1,
    state: `ended`,
    agent: `claude`,
    identifier: `VAPP-100`,
    title: `SwiftUI parity with the round-1/2 contract`,
    caption: `Done · mint · 1 h`,
    tone: `muted`,
    device: `mint`,
  },
  {
    depth: 0,
    state: `ended`,
    agent: `codex`,
    identifier: null,
    title: `Pre-cleanup release`,
    caption: `Done · macbook · 21 h`,
    tone: `muted`,
    device: `macbook`,
  },
]

function SessionRows({ size, ring }: { size: SessionRowSize; ring: string }) {
  const guides = treeGuides(RUNS.map((run) => run.depth))
  return (
    <div className="flex flex-col">
      {RUNS.map((run, index) => (
        <SessionRow
          key={run.title}
          size={size}
          agent={run.agent}
          markState={run.state}
          identifier={run.identifier}
          title={run.title}
          caption={run.caption}
          captionTone={run.tone}
          depth={run.depth}
          guide={guides[index]}
          deviceIcon={run.device === `mint` ? SERVER : LAPTOP}
          deviceName={run.device}
          active={index === 1 && size === `small`}
          ringClassName={ring}
          onClick={() => {}}
        />
      ))}
    </div>
  )
}

function SessionRowSpecimen() {
  return (
    <div className="flex flex-wrap items-start gap-8">
      <div className="flex w-[300px] max-w-full flex-col gap-1 rounded-lg bg-sidebar p-2">
        <div className="px-2 py-1 text-xs text-muted-foreground">Running</div>
        <SessionRows size="small" ring="ring-sidebar" />
      </div>
      <div className="flex w-[520px] max-w-full flex-col">
        <SessionRows size="big" ring="ring-background" />
      </div>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `session-row`,
  section: `special`,
  owner: `EXP-1248`,
  title: `Session row`,
  blurb: `ONE list item for a run, two sizes, nested by the session tree (resumes collapsed, a \`sessions_start\` child under its parent, children ALWAYS shown). Anatomy ×4: [tree guides][run mark at 12 + 14·depth][mono id · title][caption, big only][device icon]. The run mark is the agent's brand mark (Claude's spark while it works, a badge when parked, dimmed once ended), so every mark and every title at one depth aligns. NO fold chevron, NO trailing chevron, NO buttons. small = one line, 32px (the sidebar's Running); big = two lines, 52px, the caption "Building · macbook · 21 h", "Needs input · mint · 5 min" (amber), "Ready for review · …", "Done · mint · 1 h". Fixture \`list-item.json\`.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SessionRow`,
      file: `packages/ui/src/session-row.tsx`,
      note: `lists: components/session-tree.tsx (big), team/sidebar-running.tsx (small); caption lib/session-row-caption.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `run_rows::run_row`,
      file: `apps/desktop/crates/ui/src/run_rows.rs`,
      note: `small = the rail Running (sidebar.rs), big = Recent and the action Runs; caption domain/src/session_row.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `SessionRow / SessionListRow`,
      file: `apps/ios/ExpUI/Sources/SessionRow.swift`,
      note: `lists: AgentSessionsList, RecentRunsSheet, ActionDetailView Runs; caption ExpCore Domain/SessionRow.swift`,
    },
    android: {
      state: `ok`,
      symbol: `SessionRow`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/SessionRow.kt`,
      note: `lists: AgentSessionsList, RecentRunsSheet, ActionDetailScreen runs; caption domain/SessionRow.kt`,
    },
  },
  island: () => <SessionRowSpecimen />,
}
