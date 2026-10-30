import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { SealedLockedSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1459 (sealed-content.json `placeholder`): THE locked state ×4, built
// from existing parts: the transcript pill (TranscriptDivider) with the
// lock, and the chat list's italic muted preview.

const COPY = fixture.placeholder.copy

export const entry: StyleguideEntry = {
  id: `sealed-locked`,
  section: `special`,
  owner: `EXP-1459`,
  title: `Locked content`,
  blurb: `Sealed session content this install cannot open (yet) ×4 (fixtures/sealed-content.json \`placeholder\`). In a transcript (both layouts) a locked message, whatever its card kind, is THE centred glass pill of chat-transcript with the \`${fixture.placeholder.icon}\` concept: "${COPY.noKey}" while a holder has not wrapped the key for this install (it opens by itself when the key arrives), "${COPY.unreadable}" when it never opens here (a newer envelope version, malformed, refused); no time, never a bubble. In the chats list the preview line reads the same copy italic muted, like \`working\`. Titles fall back to the neutral plaintext title.`,
  status: {
    web: {
      state: `ok`,
      symbol: `TranscriptDivider kind="locked-*" / ChatList preview locked`,
      file: `packages/ui/src/surface-card.tsx`,
      note: `rows from buildSessionThread (type locked) and chatPreview (kind locked)`,
    },
    desktop: { state: `leftover`, symbol: `divider_pill`, file: `apps/desktop/crates/ui/src/surface_card.rs`, note: `the desktop UI slice of EXP-1459 ports it` },
    ios: { state: `ok`, symbol: `TranscriptDividerPill (.locked row) / ChatRowView previewItalic`, file: `apps/ios/Exponential/UI/Thread/ThreadConversation.swift`, note: `ThreadRow.locked + ChatPreviewKind.locked; specimen SealedLockedSpecimens (-uiTestingStyleguide sealed-locked)` },
    android: { state: `leftover`, symbol: `TranscriptDivider`, file: `apps/android/app/src/main/java/com/exponential/app/ui/components/ChatComponents.kt`, note: `the Android slice of EXP-1459 ports it` },
  },
  island: () => <SealedLockedSpecimen />,
}
