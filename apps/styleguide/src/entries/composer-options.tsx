import { ComposerOptionsSpecimen, ComposerSpecimen } from "./composer-specimen.tsx"
import type { StyleguideEntry } from "./types.ts"

// UI cleanup batch — the composer's options line, drawn by the REAL words it
// is made of (apps/web/src/components/launch-dialog/launch-options-line.tsx,
// session-composer.tsx): every word is a typed picker in its `inline`
// variant. EXP-1334: ONE composer — before a start the line is Device ·
// Account · Model · Plan · Repository; in a session (the thread = its
// dispatcher session) Device · Model · Plan … the context ring. EXP-1252:
// the session's Device pin is THE launch pin; a pick MOVES the session to
// that device after its turn, "Moving to <device>…" while pending. Effort, Ultracode,
// MCP servers and Computer use live in the composer's "+" menu.

function Caption({ children }: { children: string }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

export const entry: StyleguideEntry = {
  id: `composer-options`,
  section: `special`,
  owner: `EXP-1252`,
  title: `Composer options`,
  blurb: `The muted line UNDER the one composer card (EXP-1334: the start, the thread and the session view render the same composer), as the real components it is made of. Before a start: DevicePicker, AccountPicker, ModelPicker, the Plan switch, RepositoryPicker (only while a chat has several to choose from), each collapsing to plain text once there is one thing it could say. In a session: the Device pin (EXP-1252: THE launch DevicePicker; a pick moves the session to that device after its current turn, the pin reads Moving to <device>… while pending, picking the current device cancels; hidden for agents and asides), the model pin (a pick sends /model <alias>, shown at once until the run confirms), the Plan switch (live mid-session through sessions.configure) and the context ring pushed right (its usage popover on click). Effort, Ultracode, MCP servers and Computer use are rows of the composer's "+" menu (composer-menu.json ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `LaunchOptionsLine / SessionComposer`,
      file: `apps/web/src/components/launch-dialog/launch-options-line.tsx`,
      note: `the row = @exp/ui ComposerOptionsRow (packages/ui/src/composer-frame.tsx); the session line is session-composer.tsx, its Device pin session-device-pin.tsx (rules + copy lib/session-device-move.ts)`,
    },
    desktop: {
      state: `ok`,
      symbol: `ChatScreenView::render_options_row`,
      file: `apps/desktop/crates/ui/src/chat_screen.rs`,
      note: `Effort / Subagents / Ultracode moved into the "+" menu (chat_screen::plus_menu)`,
    },
    ios: {
      state: `ok`,
      symbol: `AgentOptionsRow`,
      file: `apps/ios/Exponential/UI/Agent/AgentOptionsRow.swift`,
      note: `pills in a horizontal scroll, no overflow; Effort / Subagents / Ultracode live in the "+" (ComposerPlusMenu)`,
    },
    android: {
      state: `ok`,
      symbol: `AgentOptionsRow`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/AgentOptionsRow.kt`,
      note: `same shape, no overflow; Effort / Subagents / Ultracode live in the "+" (ComposerPlusMenuSheet)`,
    },
  },
  island: () => (
    <div className="grid max-w-[40rem] gap-3">
      <Caption>Before a start (Home / New session): device · account · model · Plan · repository</Caption>
      <ComposerOptionsSpecimen phase="start" />
      <Caption>A running session: device · model · Plan (live mid-run) … the context ring</Caption>
      <ComposerOptionsSpecimen phase="running" />
      <Caption>A move pending (EXP-1252): the device pin names the target until the handoff</Caption>
      <ComposerOptionsSpecimen phase="running" moving />
      <Caption>An idle thread = its dispatcher session: the same line</Caption>
      <ComposerOptionsSpecimen phase="idle" />
      <Caption>Under the card, in the one composer</Caption>
      <ComposerSpecimen phase="running" placeholder="Type / for commands" />
    </div>
  ),
}
