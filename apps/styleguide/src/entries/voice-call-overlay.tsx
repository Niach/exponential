import voice from "@exp/etalk/fixtures/voice.json"
import voiceCall from "@exp/etalk/fixtures/voice-call.json"
import { ExponentialLogo, Pill, TooltipProvider, VoiceCallPillSpecimen, VoiceWave, type VoiceWaveActivity } from "@exp/ui"
import { renderIsland } from "@exp/ui/island"

import { escapeHtml } from "../html.ts"
import type { StyleguideEntry } from "./types.ts"

// NEW-77 — the voice overlay: the one face for push to talk (#NEW-54) and,
// later, the wake word. It is THE bar (the bulk bar's capsule, as the
// computer-use banner is), built from its own parts: `VoiceWave` (md, the
// brand mark as its child) as the glyph, the state as the bold value, what
// was heard or said as a muted item, and one pill. The IDE draws the same
// bar (`voice_overlay::capsule` on `glass_bar`).

interface Capsule {
  activity: VoiceWaveActivity
  level?: number
  bands?: number[]
  state: string
  line: string
  pill: string
  primary?: boolean
}

// #NEW-54: the four rows are the fixture's (`overlaySpecimens`), the words of
// the push-to-talk overlay (`pttCopy`); the desktop twin reads the same rows.
const CAPSULES = voice.overlaySpecimens as Capsule[]

// S-3: a call on the desktop is no capsule any more: the in-window call
// panel (the voice-call entry) while an IDE window is focused, else THE
// floating call PILL (`voice-call.json` `pill`, the call-controls entry's
// compact row), one per `surfaceSpecimens` row: the drag handle, the avatar
// small, the name over the timer (ringing: the status), VoiceWave sm, the
// round mute and the red end button (`VoiceCallPillSpecimen`, also the web
// app's `?dev=voice-call-overlay`).
function pills(): string {
  return renderIsland(
    <TooltipProvider>
      <VoiceCallPillSpecimen />
    </TooltipProvider>
  )
}

function capsule(c: Capsule): string {
  return [
    `<div class="cmp-bulk-bar" data-slot="voice-overlay">`,
    renderIsland(
      <VoiceWave activity={c.activity} level={c.level} bands={c.bands} size="md" label="Voice">
        <ExponentialLogo variant="light" />
      </VoiceWave>
    ),
    `<span class="value">${escapeHtml(c.state)}</span>`,
    `<span class="item cmp-line">${escapeHtml(c.line)}</span>`,
    renderIsland(
      <Pill size="sm" mode="action" primary={c.primary}>
        {c.pill}
      </Pill>
    ),
    `</div>`,
  ].join(``)
}

export const entry: StyleguideEntry = {
  id: `voice-call-overlay`,
  section: `special`,
  owner: `NEW-77`,
  title: `Voice overlay`,
  blurb: `The voice device's one face while it listens or speaks (push to talk #NEW-54, the wake word later): THE bar (the bulk bar's capsule, as the computer-use banner is) in a transparent, always-on-top, never-focused window at the bottom centre of the display under the pointer, 24 px above the edge. VoiceWave (md) with the brand mark as the glyph, the state as the bold value (Listening · Heard · Speaking · Microphone muted), what was heard or said as the muted item, and one pill: the agent while it listens or thinks, Stop while it speaks, Unmute while the tray mutes the microphone. It appears on key-down and fades 1.5 s after the reply. S-3: a call is no capsule: while an IDE window of the voice device is focused it is the in-window call panel (voice-call); while none is (all minimized or another app in front) it is THE floating call pill, ${voiceCall.pill.width} × ${voiceCall.pill.height} px, transparent and always on top, draggable by any point outside its buttons, its position remembered on the device (default: bottom centre of the display under the pointer, ${voiceCall.pill.marginPx} px above the edge): a drag handle, the agent's avatar small, the name over the timer (ringing: ${voiceCall.copy.calling}), VoiceWave sm, then the round ${voiceCall.copy.mute} and the red ${voiceCall.copy.endCall} buttons (call-controls, icon only). Esc and the tray's ${voice.copy.trayEndCall} end the call.`,
  status: {
    web: { state: `n/a`, note: `The voice device is a desktop; a web call draws the voice-call surface.` },
    desktop: {
      state: `ok`,
      symbol: `voice_overlay::capsule`,
      file: `apps/desktop/crates/ui/src/voice_overlay.rs`,
    },
    ios: { state: `n/a`, note: `Phones draw a call in the voice-call entry (V5); no push to talk.` },
    android: { state: `n/a`, note: `Phones draw a call in the voice-call entry (V5); no push to talk.` },
  },
  render: () =>
    `<div class="cmp-stack cmp-stack-wide">${[...CAPSULES.map(capsule), pills()].join(``)}</div>`,
}
