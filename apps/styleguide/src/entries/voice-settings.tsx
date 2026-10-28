import type { ReactNode } from "react"

import {
  agentOptions,
  languageLabel,
  railVoice,
  trayHeader,
  VOICE_COPY,
  VOICE_LANGUAGES,
  VOICE_LIMITS,
  VOICE_TEST_IDS,
  voiceDeviceOptions,
  wakeDetail,
  wakePhrasePlaceholder,
} from "@exp/domain-contract/voice"
import {
  appOptions,
  DESKTOP_AUDIO_VALUES,
  desktopAudioOptions,
  MEETING_COPY,
  MEETING_TEST_IDS,
  who,
} from "@exp/domain-contract/meeting"
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
  PickerTrigger,
  ShortcutKeys,
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
// device mid-listen: Studio, Push to talk bound to Ctrl + Space, Wake word
// on, the microphone up, a private window focused (Screen
// memory on but paused). The desktop twin paints the same rows in order.
const RAIL_VOICE = railVoice(
  {
    voiceDeviceId: `d-mint`,
    agentId: `a1`,
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

// #NEW-57: the same button while a meeting runs with the wake word off (a
// running meeting shows it too): its channels listening (the dot), the
// tooltip the tray's Meeting mode row, Meeting mode checked, Pause listening.
const RAIL_MEETING = railVoice(
  {
    voiceDeviceId: `d-mint`,
    agentId: `a1`,
    pttShortcut: ``,
    wakeWord: false,
    wakePhrase: ``,
  },
  {
    agent: `Studio`,
    muted: false,
    callAvailable: false,
    micAvailable: false,
    deviceIsVoiceDevice: true,
    meeting: { available: true, on: true, who: `You, Zoom and Slack` },
    pause: { available: true, paused: false },
    wakeListening: false,
    meetingListening: true,
    trayHosted: false,
  }
)

function railEntries(rail: typeof RAIL_VOICE): MenuEntry[] {
  return (rail?.menu ?? []).flatMap((item): MenuEntry[] => {
    if (item.kind === `separator`) return []
    if (item.kind === `check`) {
      return [{ kind: `toggle`, id: item.id, label: item.label, checked: item.checked, disabled: !item.enabled, onChange: noop }]
    }
    return [{ kind: `item`, id: item.id, label: item.label, disabled: !item.enabled, onSelect: noop }]
  })
}

// #NEW-57: a meeting runs with Desktop audio = Pick apps, Zoom and Slack
// picked, on a per-app capable voice device (Spotify plays too).
const MEETING_APPS = [
  { id: `us.zoom.xos`, name: `Zoom` },
  { id: `com.tinyspeck.slackmacgap`, name: `Slack` },
]
const LIVE_APPS = [...MEETING_APPS, { id: `com.spotify.client`, name: `Spotify` }]
const DESKTOP_AUDIO_ITEMS = desktopAudioOptions(`per_app`).map((value) => ({
  value,
  label: DESKTOP_AUDIO_VALUES.find((option) => option.value === value)?.label ?? value,
}))
const APP_ITEMS = appOptions(LIVE_APPS, MEETING_APPS)

const VoiceIcon = conceptIcon(`settings-voice`)

// S-3: the Wake word row's detail, the voice device's doctor row `wake`
// (here: it is listening).
const WAKE_DETAIL = wakeDetail({ wakeWord: true, agent: true, online: true, device: `mint`, state: `listening` })

function Caption({ children }: { children: ReactNode }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

/** One rail voice button, its pointer menu drawn open above it. */
function RailSpecimen({ rail, testId, children }: { rail: typeof RAIL_VOICE; testId: string; children: ReactNode }) {
  return (
    <div className="grid w-fit gap-2" data-testid={testId}>
      <MenuPanel entries={railEntries(rail)} density="pointer" />
      <Button variant="ghost" size="icon" className="relative size-8" aria-label={rail?.tooltip}>
        <VoiceIcon aria-hidden />
        {rail?.badge ? (
          <LiveDot tone="unread" className="pointer-events-none absolute right-1 top-1 bg-destructive" />
        ) : null}
      </Button>
      <Caption>{children}</Caption>
    </div>
  )
}

/** The rail's voice button, listening (the rail's destructive dot). */
function RailVoiceSpecimen() {
  return (
    <RailSpecimen rail={RAIL_VOICE} testId="demo-rail-voice">
      The rail's voice button while no tray host shows the tray: listening (the destructive dot), its menu open
      (railVoice), Screen memory paused by a private window. Tooltip: "{RAIL_VOICE?.tooltip}".
    </RailSpecimen>
  )
}

/** #NEW-57: the rail's voice button while a meeting runs. */
function RailMeetingSpecimen() {
  return (
    <RailSpecimen rail={RAIL_MEETING} testId="demo-rail-voice-meeting">
      While a meeting runs (the wake word off): the dot while its channels listen, Meeting mode checked and Pause
      listening in the menu. Tooltip: "{RAIL_MEETING?.tooltip}".
    </RailSpecimen>
  )
}

export const entry: StyleguideEntry = {
  id: `voice-settings`,
  section: `special`,
  owner: `NEW-67`,
  title: `Voice settings`,
  blurb: `Settings → Personal → ${VOICE_COPY.group}, after Notifications, on all four clients (NEW-67, voice.json). ONE group: ${VOICE_COPY.voiceDevice} (THE device picker's rows, None first, the member's desktops by name; phones and servers are never a voice device), ${VOICE_COPY.agent} (None first, the member's agents by label, "{name} · {team}" on a shared name); S-3: replies to the voice are always spoken (no Speak replies row). While a voice device is picked the tray group follows under "${VOICE_COPY.trayHeader}": ${VOICE_COPY.pushToTalk} (S-3: the key binding as keycaps, "${VOICE_COPY.shortcutNone}" while none; READ-ONLY on web, iOS, Android and another desktop, recorded like a game keybind on the voice device's IDE: "${VOICE_COPY.shortcutRecord}"), ${VOICE_COPY.wakeWord} (its detail why it is or is not listening, wakeDetail: the voice device's doctor row \`wake\` on every other client) and ${VOICE_COPY.wakePhrase} (placeholder "${VOICE_COPY.wakePhrasePlaceholder}"). The wake word reads off and is disabled while the device or the agent is None, and choosing None clears it (clearOnNone). Device settings no longer has a Voice group. #NEW-73: ${VOICE_COPY.language} (English / Deutsch, its trailing detail "${VOICE_COPY.languageNote}" for every language on a voice device: #NEW-74 speaks both on its GPU only) follows ${VOICE_COPY.agent}, and while a voice device is picked ${VOICE_COPY.voiceFile} (empty = "${VOICE_COPY.voiceFilePlaceholder}") and ${VOICE_COPY.voiceTranscript} follow in the same group. #NEW-68: the voice device's IDE also shows a system-tray item whose menu is OS-native (no styleguide component): ${VOICE_COPY.trayOpen} first (${VOICE_COPY.trayCall} after it, ${VOICE_COPY.trayEndCall} while a call runs), then ${VOICE_COPY.pushToTalk} · {shortcut} (an action while a key is bound, opening this page) and ${VOICE_COPY.wakeWord} · {phrase} as a check item (disabled while the device or the agent is None, as on this page; #NEW-58: Screen memory ("Screen memory · Paused" while a private window is focused; device-local, always enabled) and ${VOICE_COPY.trayMute} follow them, Mute while the microphone listens: a key bound or the wake word, and its worker up), then ${VOICE_COPY.traySettings} (opens this page) and ${VOICE_COPY.trayQuit}. #NEW-97: while no tray host shows that item (GNOME without AppIndicator) and the wake word is on, the IDE rail's footer carries a voice button instead, beside Computer: the same ghost square with the settings-voice glyph, the rail's destructive dot while the wake word holds the microphone open, the tooltip "Listening for “{phrase}”" (or "Microphone muted"), and a pointer menu with the tray's rows minus ${VOICE_COPY.trayOpen} and ${VOICE_COPY.trayQuit} (railVoice). The specimen under the page shows that button listening with its menu open, Screen memory paused by a private window. #NEW-57 (meeting.json): directly under the tray group, in a group of their own with no header (so ${VOICE_COPY.wakePhrase} and its inline error close the tray group), come ${MEETING_COPY.meeting} (turning it on opens THE consent prompt, entry meeting-consent; while on its caption under the label reads who is transcribed), ${MEETING_COPY.desktopAudio} (Off / Everything / Pick apps / Everything except, as far as the voice device can capture) and, while it picks or leaves out apps and the voice device can capture per app, ${MEETING_COPY.apps} (a multi-picker over the apps playing audio, then the stored ones; its value the picked names or "${MEETING_COPY.appsNone}"). Shown: a meeting running for "You, Zoom and Slack". A second specimen under the page pictures the rail's voice button while a meeting runs with the wake word off (a running meeting shows it too): the dot while its channels listen, the tooltip "${MEETING_COPY.meeting} · {who}", Meeting mode checked and ${MEETING_COPY.pause} in its menu; the tray icon carries the same dot. A meeting stops itself after 4 hours (an info toast "${MEETING_COPY.autoStopped.replace(`{hours}`, `4`)}" and, in the IDE, a system notification).`,
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
        {/* S-3: read-only off the voice device: the binding as keycaps. */}
        <div
          data-slot="glass-value-row"
          data-testid={`demo-${VOICE_TEST_IDS.shortcut}`}
          className="flex items-center gap-3 px-4 py-3"
        >
          <span className="flex-1 text-sm font-normal">{VOICE_COPY.pushToTalk}</span>
          <ShortcutKeys value="ctrl+space" />
        </div>
        <GlassToggleRow
          id={`demo-${VOICE_TEST_IDS.wakeWord}`}
          label={VOICE_COPY.wakeWord}
          description={WAKE_DETAIL ?? undefined}
          checked
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
      <GlassGroup className="mt-3">
        <GlassToggleRow
          id={`demo-${MEETING_TEST_IDS.meeting}`}
          label={MEETING_COPY.meeting}
          description={who(`include`, MEETING_APPS, `per_app`)}
          checked
          onCheckedChange={noop}
        />
        <Picker
          mode="single"
          triggerVariant="row"
          width="md"
          mobileTitle={MEETING_COPY.desktopAudio}
          value="include"
          items={DESKTOP_AUDIO_ITEMS}
          onChange={noop}
        />
        <Picker
          mode="multi"
          width="md"
          mobileTitle={MEETING_COPY.apps}
          value={MEETING_APPS.map((app) => app.id)}
          items={APP_ITEMS}
          onChange={noop}
          trigger={
            <PickerTrigger
              variant="row"
              label={MEETING_COPY.apps}
              value={MEETING_APPS.map((app) => app.name).join(`, `)}
            />
          }
        />
      </GlassGroup>
      <div className="mt-8">
        <RailVoiceSpecimen />
      </div>
      <div className="mt-8">
        <RailMeetingSpecimen />
      </div>
    </SubShellHost>
  ),
}
