import { contract } from "@exp/domain-contract"
import { consentBody, MEETING_COPY, MEETING_TEST_IDS, who } from "@exp/domain-contract/meeting"

import { PromptSpecimen } from "./dialog-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

// NEW-57 (meeting.json): THE consent dialog Meeting mode opens before a
// meeting starts. The app's `Prompt` portals its card, so the specimen draws
// the same `PromptLayout` in the dialog frame; every word is the fixture's
// (`copy.consentTitle`, `consentBody(who, agent, days)`, Cancel / Start).

const WHO = who(`include`, [{ id: `z`, name: `Zoom` }, { id: `s`, name: `Slack` }], `per_app`)

export const entry: StyleguideEntry = {
  id: `meeting-consent`,
  section: `special`,
  owner: `NEW-57`,
  title: `Meeting consent`,
  blurb: `What turning Meeting mode on asks first (NEW-57, meeting.json), on all four clients: Settings → Voice's ${MEETING_COPY.meeting} row and the tray's check item never start a meeting by themselves, they open THIS prompt (the shared Prompt: one question, the body naming who is transcribed (who), the voice agent and the server retention (contract.meeting.retentionDays), then ${MEETING_COPY.cancel} and the primary ${MEETING_COPY.start}). ${MEETING_COPY.start} = voice.startMeeting {consent: true}; ${MEETING_COPY.cancel}, Esc and the scrim leave it off. Turning it off never asks.`,
  status: {
    web: {
      state: `ok`,
      symbol: `VoiceSettingsCard`,
      file: `apps/web/src/components/account/voice-settings-card.tsx`,
      note: `the shared Prompt over the Meeting mode row`,
    },
    desktop: {
      state: `ok`,
      symbol: `render`,
      file: `apps/desktop/crates/ui/src/styleguide/entries/meeting_consent.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `VoiceSettingsPage`,
      file: `apps/ios/Exponential/UI/Settings/VoiceSettingsPage.swift`,
    },
    android: {
      state: `ok`,
      symbol: `VoiceSettingsScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/settings/VoiceSettingsScreen.kt`,
    },
  },
  island: () => (
    <div className="grid gap-6">
      <PromptSpecimen
        caption={`${MEETING_COPY.meeting} turned on`}
        title={MEETING_COPY.consentTitle}
        body={consentBody(WHO, `Ops`, contract.meeting.retentionDays)}
        actions={[
          { label: MEETING_COPY.cancel, role: `cancel` },
          { label: MEETING_COPY.start, role: `primary`, testId: `demo-${MEETING_TEST_IDS.consentStart}` },
        ]}
      />
    </div>
  ),
}
