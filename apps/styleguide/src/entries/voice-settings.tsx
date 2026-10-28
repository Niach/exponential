import type { ReactNode } from "react"

import {
  agentOptions,
  languageLabel,
  railVoice,
  trayHeader,
  VOICE_COPY,
  VOICE_LANGUAGES,
  VOICE_LIMITS,
  VOICE_SHORTCUTS,
  VOICE_TEST_IDS,
  voiceDeviceOptions,
  wakePhrasePlaceholder,
} from "@exp/domain-contract/voice"
import {
  Button,
  conceptIcon,
  devicePickerItems,
  GlassGroup,
  GlassInputRow,
  GlassSectionHeader,
  GlassToggleRow,
  LiveDot,
  type MenuEntry,
  MenuPanel,
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

// The card reads every language through `languageLabel` while a voice device
// is picked (#NEW-73 / #NEW-74): "English · Needs a GPU on mint".
const LANGUAGES = VOICE_LANGUAGES.map((language) => ({
  value: language.value,
  label: languageLabel(language.value, `mint`),
}))

// #NEW-97 / #NEW-58: the IDE rail's voice button (desktop `rail_voice_entry`)
// with its pointer menu OPEN, the rows `railVoice` derives for the voice
// device mid-listen: Studio, Push to talk on Ctrl + Space, Wake word on,
// Speak replies on, the microphone up, a private window focused (Screen
// memory on but paused). The desktop twin paints the same rows in order.
const RAIL_VOICE = railVoice(
  {
    voiceDeviceId: `d-mint`,
    agentId: `a1`,
    speakReplies: true,
    pushToTalk: true,
    pttShortcut: `ctrl+space`,
    wakeWord: true,
    wakePhrase: ``,
  },
  {
    agent: `Studio`,
    muted: false,
    callAvailable: false,
    micAvailable: true,
    deviceIsVoiceDevice: true,
    screenMemory: { available: true, on: true, paused: true },
    wakeListening: true,
    trayHosted: false,
  }
)

const RAIL_VOICE_ENTRIES: MenuEntry[] = (RAIL_VOICE?.menu ?? []).flatMap((item): MenuEntry[] => {
  if (item.kind === `separator`) return []
  if (item.kind === `check`) {
    return [{ kind: `toggle`, id: item.id, label: item.label, checked: item.checked, disabled: !item.enabled, onChange: noop }]
  }
  return [{ kind: `item`, id: item.id, label: item.label, disabled: !item.enabled, onSelect: noop }]
})

const VoiceIcon = conceptIcon(`settings-voice`)

function Caption({ children }: { children: ReactNode }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

/** The rail's voice button, listening (the rail's destructive dot), its
 *  pointer menu drawn open above it. */
function RailVoiceSpecimen() {
  return (
    <div className="grid w-fit gap-2" data-testid="demo-rail-voice">
      <MenuPanel entries={RAIL_VOICE_ENTRIES} density="pointer" />
      <Button variant="ghost" size="icon" className="relative size-8" aria-label={RAIL_VOICE?.tooltip}>
        <VoiceIcon aria-hidden />
        {RAIL_VOICE?.badge ? (
          <LiveDot tone="unread" className="pointer-events-none absolute right-1 top-1 bg-destructive" />
        ) : null}
      </Button>
      <Caption>
        The rail's voice button while no tray host shows the tray: listening (the destructive dot), its menu open
        (railVoice), Screen memory paused by a private window. Tooltip: "{RAIL_VOICE?.tooltip}".
      </Caption>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `voice-settings`,
  section: `special`,
  owner: `NEW-67`,
  title: `Voice settings`,
  blurb: `Settings → Personal → ${VOICE_COPY.group}, after Notifications, on all four clients (NEW-67, voice.json). ONE group: ${VOICE_COPY.voiceDevice} (THE device picker's rows, None first, the member's desktops by name; phones and servers are never a voice device), ${VOICE_COPY.agent} (None first, the member's agents by label, "{name} · {team}" on a shared name) and ${VOICE_COPY.speakReplies}. While a voice device is picked the tray group follows under "${VOICE_COPY.trayHeader}": ${VOICE_COPY.pushToTalk}, ${VOICE_COPY.shortcut}, ${VOICE_COPY.wakeWord} and ${VOICE_COPY.wakePhrase} (placeholder "${VOICE_COPY.wakePhrasePlaceholder}"). Every switch reads off and is disabled while the device or the agent is None, and choosing None clears them (clearOnNone). Device settings no longer has a Voice group. #NEW-73: ${VOICE_COPY.language} (English / Deutsch, its trailing detail "${VOICE_COPY.languageNote}" for every language on a voice device: #NEW-74 speaks both on its GPU only) follows ${VOICE_COPY.speakReplies}, and while a voice device is picked ${VOICE_COPY.voiceFile} (empty = "${VOICE_COPY.voiceFilePlaceholder}") and ${VOICE_COPY.voiceTranscript} follow in the same group. #NEW-68: the voice device's IDE also shows a system-tray item whose menu is OS-native (no styleguide component): ${VOICE_COPY.trayOpen} first (${VOICE_COPY.trayCall} after it once calls land, V4), then ${VOICE_COPY.speakReplies}, ${VOICE_COPY.pushToTalk} · {shortcut} and ${VOICE_COPY.wakeWord} · {phrase} as check items (disabled while the device or the agent is None, as on this page; #NEW-58: Screen memory ("Screen memory · Paused" while a private window is focused; device-local, always enabled) and ${VOICE_COPY.trayMute} follow them, Mute while the microphone listens: Push to talk on and its worker up), then ${VOICE_COPY.traySettings} (opens this page) and ${VOICE_COPY.trayQuit}. #NEW-97: while no tray host shows that item (GNOME without AppIndicator) and the wake word is on, the IDE rail's footer carries a voice button instead, beside Computer: the same ghost square with the settings-voice glyph, the rail's destructive dot while the wake word holds the microphone open, the tooltip "Listening for “{phrase}”" (or "Microphone muted"), and a pointer menu with the tray's rows minus ${VOICE_COPY.trayOpen} and ${VOICE_COPY.trayQuit} (railVoice). The specimen under the page shows that button listening with its menu open, Screen memory paused by a private window.`,
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
        <Picker
          mode="single"
          triggerVariant="row"
          width="md"
          mobileTitle={VOICE_COPY.language}
          value={LANGUAGES[0].value}
          items={LANGUAGES}
          onChange={noop}
        />
        <GlassInputRow
          id={`demo-${VOICE_TEST_IDS.voiceFile}`}
          label={VOICE_COPY.voiceFile}
          placeholder={VOICE_COPY.voiceFilePlaceholder}
          maxLength={VOICE_LIMITS.voiceFile}
          value=""
          onChange={noop}
        />
        <GlassInputRow
          id={`demo-${VOICE_TEST_IDS.voiceTranscript}`}
          label={VOICE_COPY.voiceTranscript}
          placeholder={VOICE_COPY.voiceTranscriptPlaceholder}
          maxLength={VOICE_LIMITS.voiceTranscript}
          value=""
          onChange={noop}
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
          maxLength={64}
          value=""
          onChange={noop}
        />
      </GlassGroup>
      <div className="mt-8">
        <RailVoiceSpecimen />
      </div>
    </SubShellHost>
  ),
}
