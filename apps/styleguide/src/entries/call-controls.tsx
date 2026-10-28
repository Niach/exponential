import call from "@exp/domain-contract/fixtures/voice-call.json"
import { CallControlsSpecimen, TooltipProvider } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// S-3 — THE round call buttons (voice-call.json PHONE-CALL UI), WhatsApp /
// Teams style, icon only: `CallButton` (the shadcn Button at eUI's round
// `icon-lg` size, 56 px) in its tones (neutral = the muted surface fill,
// pressed = inverted, destructive = the red end button; the ring's Cancel
// writes its label under it), `CallControls` (callView's `buttons` in
// order) and the compact row, THE desktop floating call pill (`pill`):
// `@exp/ui`'s `CallControlsSpecimen`, also the web app's `?dev=call-controls`.

export const entry: StyleguideEntry = {
  id: `call-controls`,
  section: `general`,
  owner: `S-3`,
  title: `Call controls`,
  blurb: `The round icon-only buttons of a call, the same on all four clients: 56 px circles (the pill: 36 px), the icon 24 px. Neutral = the muted surface fill (${call.copy.mute}, ${call.copy.speaker}); a toggle that is on (${call.copy.unmute}) paints inverted, the foreground fill with the background's icon; the end button is THE red one (the destructive fill at full strength). Every label is the tooltip and the accessibility label, never words beside the icon, except the ring's end button: "${call.copy.cancel}" written under it. The row lays out callView's buttons in order (phones: mute, speaker, end; web and desktop: mute, end; ringing: end alone) 32 px apart. The compact row is the desktop's floating call pill (${call.pill.width} × ${call.pill.height} px, fixed): a drag handle, the agent's avatar small, the name over the timer, VoiceWave sm, then mute and end at 36 px.`,
  status: {
    web: { state: `ok`, symbol: `CallButton · CallControls · VoiceCallPill`, file: `packages/ui/src/call-controls.tsx` },
    desktop: { state: `ok`, note: `The desktop call UI's round call buttons and the floating pill (styleguide twin call-controls).` },
    ios: { state: `ok`, note: `The phone call screen's round call buttons (VoiceCallScreen).` },
    android: {
      state: `ok`,
      symbol: `CallControls`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/CallControls.kt`,
    },
  },
  island: () => (
    <TooltipProvider>
      <CallControlsSpecimen />
    </TooltipProvider>
  ),
}
