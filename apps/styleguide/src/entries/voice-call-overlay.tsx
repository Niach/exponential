import voice from "@exp/domain-contract/fixtures/voice.json"
import voiceCall from "@exp/domain-contract/fixtures/voice-call.json"
import { ExponentialLogo, Pill, VoiceWave, type VoiceWaveActivity } from "@exp/ui"
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
  /** NEW-55: a call's Mute / Unmute pill, before End call. */
  lead?: string
}

// #NEW-54: the four rows are the fixture's (`overlaySpecimens`), the words of
// the push-to-talk overlay (`pttCopy`); the desktop twin reads the same rows.
const CAPSULES = voice.overlaySpecimens as Capsule[]

// NEW-55: a call's capsules (`voice-call.json` `surfaceSpecimens`): the lead
// pill Mute / Unmute, then End call as the one primary pill.
const CALL_CAPSULES: Capsule[] = voiceCall.surfaceSpecimens.map((row) => ({
  activity: row.activity as VoiceWaveActivity,
  level: row.level,
  bands: row.bands,
  state: row.state,
  line: row.line,
  pill: voiceCall.copy.endCall,
  primary: true,
  lead: row.mute,
}))

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
    c.lead
      ? renderIsland(
          <Pill size="sm" mode="action">
            {c.lead}
          </Pill>
        )
      : ``,
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
  blurb: `The voice device's one face while it listens or speaks (push to talk #NEW-54, the wake word later): THE bar (the bulk bar's capsule, as the computer-use banner is) in a transparent, always-on-top, never-focused window at the bottom centre of the display under the pointer, 24 px above the edge. VoiceWave (md) with the brand mark as the glyph, the state as the bold value (Listening · Heard · Speaking · Microphone muted), what was heard or said as the muted item, and one pill: the agent while it listens or thinks, Stop while it speaks, Unmute while the tray mutes the microphone. It appears on key-down and fades 1.5 s after the reply. A call (V4, NEW-55) keeps the capsule until End call: the lead pill Mute (Unmute while muted) and End call as the primary pill.`,
  status: {
    web: { state: `n/a`, note: `The voice device is a desktop; web streams a call to it later (V6).` },
    desktop: {
      state: `ok`,
      symbol: `voice_overlay::capsule`,
      file: `apps/desktop/crates/ui/src/voice_overlay.rs`,
    },
    ios: { state: `n/a`, note: `Phones draw a call in the voice-call entry (V5); no push to talk.` },
    android: { state: `n/a`, note: `Phones draw a call in the voice-call entry (V5); no push to talk.` },
  },
  render: () => `<div class="cmp-stack cmp-stack-wide">${[...CAPSULES, ...CALL_CAPSULES].map(capsule).join(``)}</div>`,
}
