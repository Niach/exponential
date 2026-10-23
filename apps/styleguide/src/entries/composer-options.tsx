import { ComposerOptionsSpecimen, ComposerSessionRowSpecimen, ComposerSpecimen } from "./composer-specimen.tsx"
import type { StyleguideEntry } from "./types.ts"

// UI cleanup batch — the composer's options, drawn by the REAL words they
// are made of (apps/web/src/components/launch-dialog/launch-options-line.tsx,
// session-composer.tsx): every word is a typed picker in its `inline`
// variant. EXP-1334: ONE composer. N-12 (owner 2026-10-22, the iOS row ×4):
// before a start the line UNDER the card is Device · Account · Model · Plan ·
// Repository; in a session (the thread = its root session) the Device pin as
// its glyph alone · Model · Plan sit IN the card's tool row after the "+"
// (the only tool: no inline image/file/#/emoji tools) and the context ring
// beside Send ⇄ Stop; nothing under the card. EXP-1252: the session's Device
// pin is THE launch pin; a pick MOVES the session to that device. Effort,
// Ultracode, MCP servers, Computer use and Add file live in the "+" menu.

function Caption({ children }: { children: string }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

export const entry: StyleguideEntry = {
  id: `composer-options`,
  section: `special`,
  owner: `EXP-1252`,
  title: `Composer options`,
  blurb: `The one composer's options (EXP-1334: the start, the thread and the session view render the same composer), as the real components they are made of. Before a start the muted line UNDER the card: DevicePicker, AccountPicker, ModelPicker, the Plan switch, RepositoryPicker (only while a chat has several to choose from), each collapsing to plain text once there is one thing it could say. N-12, the iOS row ×4: in a session the pins sit IN the card's tool row after the "+" (the only tool): the Device pin as its glyph alone (EXP-1252: THE launch DevicePicker; a pick moves the session to that device: a queued one re-targets, a live one ends here and continues there; hidden for agents), the model pin (a pick sends /model <alias>, shown at once until the run confirms), the Plan switch (live mid-session through sessions.configure); the context ring (16px, its usage popover on click) sits beside Send ⇄ Stop; nothing under the card. Effort, Ultracode, MCP servers, Computer use and Add file are rows of the "+" menu (composer-menu.json ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `LaunchOptionsLine / SessionComposer`,
      file: `apps/web/src/components/launch-dialog/launch-options-line.tsx`,
      note: `the start line = @exp/ui ComposerOptionsRow (packages/ui/src/composer-frame.tsx); the session row is session-composer.tsx's tool row, its Device pin session-device-pin.tsx (PickerTrigger iconOnly; rules + copy lib/session-device-move.ts)`,
    },
    desktop: {
      state: `ok`,
      symbol: `session_composer_card`,
      file: `apps/desktop/crates/ui/src/composer.rs`,
      note: `the start line: ChatScreenView::render_options_row (chat_screen.rs); the glyph-only device pin: device_move.rs session_device_pin`,
    },
    ios: {
      state: `ok`,
      symbol: `SessionComposerFooter`,
      file: `apps/ios/Exponential/UI/Session/SessionComposerFooter.swift`,
      note: `the session view and the thread (ThreadComposer) share the in-card row; the device pin icon-only (SessionDevicePinView iconOnly), the Plan switch SessionPlanSwitch`,
    },
    android: {
      state: `ok`,
      symbol: `SessionComposer`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentSessionScreen.kt`,
      note: `N-12: "+" · the Device glyph · the model · Plan … the 16dp ring · Send ⇄ Stop in the card's row (ExpandedSteerComposer); nothing under the card in a session or a thread`,
    },
  },
  island: () => (
    <div className="grid max-w-[40rem] gap-3">
      <Caption>Before a start (Home / New session): device · account · model · Plan · repository, under the card</Caption>
      <ComposerOptionsSpecimen phase="start" />
      <Caption>A running session: the pins in the card's tool row — the device glyph · model · Plan (live mid-run)</Caption>
      <ComposerSessionRowSpecimen phase="running" />
      <Caption>An idle thread = its root session: the same row</Caption>
      <ComposerSessionRowSpecimen phase="idle" />
      <Caption>The one composer in a session: the "+", the pins, the context ring beside Stop; nothing under the card</Caption>
      <ComposerSpecimen phase="running" placeholder="Type / for commands" />
    </div>
  ),
}
