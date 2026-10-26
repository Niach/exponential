import {
  agentOptions,
  trayHeader,
  VOICE_COPY,
  VOICE_SHORTCUTS,
  VOICE_TEST_IDS,
  voiceDeviceOptions,
  wakePhrasePlaceholder,
} from "@exp/domain-contract/voice"
import {
  devicePickerItems,
  GlassGroup,
  GlassInputRow,
  GlassSectionHeader,
  GlassToggleRow,
  Picker,
  SubShellHost,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// NEW-67 (voice plan v3, V1): Settings → Personal → Voice, the member's ONE
// voice settings row (fixtures/voice.json ×4). Built from the same rows the
// web page uses (`VoiceSettingsCard`): the options come from the fixture's
// own builders, so the specimen reads exactly what every client derives.

const noop = (): void => {}

const DEVICES = voiceDeviceOptions([
  { deviceId: `d-mint`, name: `mint`, kind: `desktop` },
  { deviceId: `d-mac`, name: `macbook`, kind: `desktop` },
  { deviceId: `d-phone`, name: `iPhone`, kind: `phone` },
]).flatMap((option) => (option.id === null ? [] : [{ id: option.id, name: option.label, kind: `desktop` }]))

const AGENTS = agentOptions([
  { id: `a1`, name: `Studio`, team: `Acme` },
  { id: `a2`, name: `Helper`, team: `Acme` },
]).flatMap((option) => (option.id === null ? [] : [{ value: option.id, label: option.label }]))

export const entry: StyleguideEntry = {
  id: `voice-settings`,
  section: `special`,
  owner: `NEW-67`,
  title: `Voice settings`,
  blurb: `Settings → Personal → ${VOICE_COPY.group}, after Notifications, on all four clients (NEW-67, voice.json). ONE group: ${VOICE_COPY.voiceDevice} (THE device picker's rows, None first, the member's desktops by name; phones and servers are never a voice device), ${VOICE_COPY.agent} (None first, the member's agents by label, "{name} · {team}" on a shared name) and ${VOICE_COPY.speakReplies}. While a voice device is picked the tray group follows under "${VOICE_COPY.trayHeader}": ${VOICE_COPY.pushToTalk}, ${VOICE_COPY.shortcut}, ${VOICE_COPY.wakeWord} and ${VOICE_COPY.wakePhrase} (placeholder "${VOICE_COPY.wakePhrasePlaceholder}"). Every switch reads off and is disabled while the device or the agent is None, and choosing None clears them (clearOnNone). Device settings no longer has a Voice group.`,
  status: {
    web: {
      state: `ok`,
      symbol: `VoiceSettingsCard`,
      file: `apps/web/src/components/account/voice-settings-card.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `VoicePrefsPane`,
      file: `apps/desktop/crates/ui/src/settings/voice_prefs.rs`,
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
    <SubShellHost className="w-full">
      <GlassGroup data-testid={`demo-${VOICE_TEST_IDS.page}`}>
        <Picker
          mode="single"
          triggerVariant="row"
          width="md"
          mobileTitle={VOICE_COPY.voiceDevice}
          value="d-mint"
          items={devicePickerItems(DEVICES)}
          noneLabel={VOICE_COPY.voiceDeviceNone}
          onNone={noop}
          onChange={noop}
        />
        <Picker
          mode="single"
          triggerVariant="row"
          width="md"
          mobileTitle={VOICE_COPY.agent}
          value="a1"
          items={AGENTS}
          noneLabel={VOICE_COPY.agentNone}
          onNone={noop}
          onChange={noop}
        />
        <GlassToggleRow
          id={`demo-${VOICE_TEST_IDS.speakReplies}`}
          label={VOICE_COPY.speakReplies}
          checked
          onCheckedChange={noop}
        />
      </GlassGroup>
      <GlassSectionHeader label={trayHeader(`mint`)} />
      <GlassGroup>
        <GlassToggleRow
          id={`demo-${VOICE_TEST_IDS.pushToTalk}`}
          label={VOICE_COPY.pushToTalk}
          checked={false}
          onCheckedChange={noop}
        />
        <Picker
          mode="single"
          triggerVariant="row"
          width="md"
          mobileTitle={VOICE_COPY.shortcut}
          value={VOICE_SHORTCUTS[0].value}
          items={VOICE_SHORTCUTS}
          onChange={noop}
        />
        <GlassToggleRow
          id={`demo-${VOICE_TEST_IDS.wakeWord}`}
          label={VOICE_COPY.wakeWord}
          checked={false}
          onCheckedChange={noop}
        />
        <GlassInputRow
          id={`demo-${VOICE_TEST_IDS.wakePhrase}`}
          label={VOICE_COPY.wakePhrase}
          placeholder={wakePhrasePlaceholder(`Studio`)}
          value=""
          onChange={noop}
        />
      </GlassGroup>
    </SubShellHost>
  ),
}
