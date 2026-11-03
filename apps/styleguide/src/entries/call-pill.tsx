import call from "@exp/etalk/fixtures/voice-call.json"
import { TooltipProvider, VoiceCallPillSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1681 — THE floating call pill (voice-call.json `pill`, testId `pill`):
// one per `surfaceSpecimens` row, as the desktop draws it while no IDE
// window is focused and the phones draw a minimized call. `@exp/ui`'s
// `VoiceCallPillSpecimen`, also the web app's `?dev=call-pill`; the IDE twin
// paints `voice_overlay::call_pill`.

export const entry: StyleguideEntry = {
  id: `call-pill`,
  section: `special`,
  owner: `EXP-1681`,
  title: `Call pill`,
  blurb: `A running call, small and floating: ${call.pill.width} × ${call.pill.height} px (on a phone never wider than the screen minus ${call.pill.marginPx} px each side), a drag handle, the agent's avatar small, the name over the timer (ringing: ${call.copy.calling}), VoiceWave sm, then the round ${call.copy.mute} and the red ${call.copy.endCall} buttons (call-controls, compact; ringing: ${call.copy.cancel} alone). Draggable by any point outside its buttons; its position is remembered on the device. The desktop shows it while no IDE window is focused; the phones while a call is minimized (the ${call.copy.minimize} button at the call screen's top leading corner, a swipe down, Android's back), above every screen, ${call.pill.phoneBottomPx} px above the bottom by default; a tap opens the call screen again. Web has no pill: its call is the full-height overlay.`,
  status: {
    web: { state: `n/a`, note: `A web call is the full-height overlay; the specimen is VoiceCallPill (packages/ui call-controls).` },
    desktop: {
      state: `ok`,
      symbol: `voice_overlay::call_pill`,
      file: `apps/desktop/crates/etalk-gpui/src/voice_overlay.rs`,
    },
    ios: { state: `leftover`, note: `Lands with the phone pill (EXP-1681): a minimized call has no surface yet.` },
    android: { state: `leftover`, note: `Lands with the phone pill (EXP-1681): a minimized call has no surface yet.` },
  },
  island: () => (
    <TooltipProvider>
      <VoiceCallPillSpecimen />
    </TooltipProvider>
  ),
}
