import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-162 — the voice level meter: Exponential UI's VoiceWave `variant:
// bars` (round-5 contract §5), painted by the real React renderer from
// fixtures/specimens.json `exponential-ui-voice-wave` (the four bar sizes).

export const entry: StyleguideEntry = {
  id: `voice-level`,
  section: `general`,
  owner: `VAPP-162`,
  title: `Voice level`,
  blurb: `The flat voice level for a row or a bar where the round wave has no room: Exponential UI's VoiceWave with \`variant: bars\`, 20 rounded vertical bars centred on the line (sm 16 px tall, 2 px bars and gaps, 78 wide; md 24/3; lg 40/4; xl 56/5). The same inputs as the ring: activity, level and up to 8 bands. Listening and speaking drive the bars by the level, thinking holds a low even ripple, idle and muted lie flat in the muted colour; a centre envelope keeps the middle tallest and a phase moves them while it is live (still under reduced motion). The heights are the core's (voiceWaveBars, fixtures/voice-wave.json bars). No child: one stays in the tree, unpainted.`,
  status: {
    web: { state: `ok`, symbol: `VoiceWaveNative`, file: `packages/exponential-ui-react/src/natives/feedback.tsx`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `voice_wave_bars`, file: `apps/desktop/crates/eui-gpui/src/paint/natives.rs` },
    ios: { state: `ok`, symbol: `VoiceWaveBarsLeaf`, file: `packages/exponential-ui-swift/Sources/ExponentialUI/Views/Paint/VoiceWave.swift` },
    android: { state: `ok`, symbol: `drawVoiceWaveBars`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/Round5.kt` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-voice-wave" cases={VOICE_LEVEL_CASES} />,
}

/** The bars at sm, md, lg and xl. */
const VOICE_LEVEL_CASES = [6, 7, 8, 9] as const
