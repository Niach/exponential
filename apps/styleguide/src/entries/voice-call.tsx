import call from "@exp/etalk/fixtures/voice-call.json"
import { ExponentialUiTwin, TooltipProvider, VoiceCallSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// NEW-55 / S-3 — THE call surface, phone-call style (WhatsApp / Teams): on
// phones full screen (V5 #NEW-69), on web a full-height overlay over the
// agent chat (V6), on the desktop the in-window call panel. The rows are
// `fixtures/voice-call.json` `surfaceSpecimens` (`callView(state, platform)`
// plus the timer): ringing (Calling… / Getting ready…, only the red end
// button labelled Cancel under it), then in call (the timer, the caption,
// the round mute / speaker / red end buttons: the `call-controls` entry).
// The surface is `@exp/ui`'s `VoiceCallSurface`, the one web's call draws
// (the specimen `VoiceCallSpecimen`, also the web app's `?dev=voice-call`).

export const entry: StyleguideEntry = {
  id: `voice-call`,
  section: `special`,
  owner: `NEW-55`,
  title: `Voice call`,
  blurb: `A call with an agent, streamed to the member's voice device, like a phone call. A call RINGS first (a random ${call.constants.minRingMs / 1000} to ${(call.constants.minRingMs + call.constants.ringJitterMs) / 1000} s, longer until the voice device is ready; a ${call.constants.ringToneMs / 1000} s ring tone every ${(call.constants.ringToneMs + call.constants.ringGapMs) / 1000} s on the calling client) while the voice device loads its speech models and opens the microphone; then the agent speaks first: the device answers "${call.copy.greeting.en}". Top to bottom: the agent's avatar large (the Assistant: the Exponential logo) with the ring of its activity (ringing a soft halo pulse, listening the green ring glowing with the level, speaking the wave, thinking dim, muted none), the agent's name, the status (${call.copy.calling} · ${call.copy.gettingReady}) or once answered the call's timer (mm:ss), the caption (what was heard, quoted, or the sentence being said; at most 4 lines), and at the bottom the round icon-only call buttons (call-controls): while ringing only the red end button with "${call.copy.cancel}" under it, in call ${call.copy.mute} (toggled: ${call.copy.unmute}, inverted), ${call.copy.speaker} (phones only: loudspeaker vs earpiece) and the red ${call.copy.endCall}, their labels as tooltips. Phones show it full screen, web as a full-height overlay over the agent chat (no speaker button), the desktop as the call panel over the agent chat's body (and the floating pill while no window is focused: voice-call-overlay). It opens from the telephone in the agent chat's header and stays until End call. Exponential UI twin (VAPP-162): CallSurface, this screen as an SDK macro (state ringing · connecting · talking · ended, the wave, avatar, name, status and caption, the round mute / speaker / red end buttons, ringing = Cancel alone; mute, speaker and hangUp events, the host ticks the timer), painted below from specimens.json \`exponential-ui-call-surface\`.`,
  status: {
    web: { state: `ok`, symbol: `VoiceCallSurface`, file: `packages/ui/src/voice-call-card.tsx` },
    desktop: { state: `ok`, note: `The in-window call panel over the agent chat's body (the desktop call UI).` },
    ios: {
      state: `ok`,
      symbol: `VoiceCallScreen`,
      file: `apps/ios/Exponential/UI/Voice/VoiceCallScreen.swift`,
      note: `Full screen over the app background (#NEW-69).`,
    },
    android: {
      state: `ok`,
      symbol: `VoiceCallSurface`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/call/VoiceCallSurface.kt`,
      note: `Full screen over the app background (#NEW-69).`,
    },
  },
  island: () => (
    <TooltipProvider>
      <ExponentialUiTwin name="CallSurface" id="exponential-ui-call-surface">
        <VoiceCallSpecimen />
      </ExponentialUiTwin>
    </TooltipProvider>
  ),
}
