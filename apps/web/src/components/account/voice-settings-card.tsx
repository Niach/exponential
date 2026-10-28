// NEW-67 (voice plan v3, V1): Settings → Voice, the member's ONE voice
// settings row (`fixtures/voice.json`). One group: the voice device (THE
// device picker's rows over `voiceDeviceOptions`, None first), the agent
// (`agentOptions`, None first); while a voice device is picked the tray group
// follows under `trayHeader`. #NEW-73: Language (`languages`, each read
// through `languageLabel`) follows the agent,
// and while a voice device is picked the voice file + its transcript. Every switch reads off
// and is disabled while the device or the agent is None, and choosing None
// clears them (`clearOnNone`). Each change is an optimistic `voice.update`
// that reverts with the failed-action toast (feedback-states.json).
// #NEW-57 (meeting.json): after Wake phrase the tray group gains Meeting mode
// (ON opens THE consent `Prompt`, Start = `voice.startMeeting`; OFF =
// `voice.stopMeeting` at once; its detail = `who`), Desktop audio over the
// voice device's `doctor.desktopAudio` capability and, while it picks or
// leaves out apps, the Apps multi-picker over `appOptions(live, stored)`.
// S-3: speak-back is always on (no Speak replies row) and push to talk is a
// key binding recorded on the voice device's IDE: here the Push to talk row
// is READ-ONLY, the binding as keycaps (`shortcutLabel`, None while empty).
// The Wake word row's detail says why it is or is not listening: the voice
// device's doctor row `wake` verbatim, else `wakeDetail` (no agent, offline,
// not reported yet).
import { useEffect, useMemo, useRef, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import {
  agentOptions,
  clearOnNone,
  languageLabel,
  trayHeader,
  VOICE_COPY,
  VOICE_LANGUAGES,
  VOICE_LIMITS,
  VOICE_TEST_IDS,
  voiceDeviceOptions,
  voiceTextInput,
  wakeDetail,
  wakePhraseError,
  wakePhrasePlaceholder,
  type VoiceAgentLike,
} from "@exp/domain-contract/voice"
import { contract } from "@exp/domain-contract"
import {
  appOptions,
  consentBody,
  DESKTOP_AUDIO_VALUES,
  desktopAudioOptions,
  MEETING_COPY,
  MEETING_LIMITS,
  MEETING_TEST_IDS,
  who,
  type AudioApp,
  type DesktopAudio,
  type DesktopAudioCapability,
} from "@exp/domain-contract/meeting"
import {
  devicePickerItems,
  FieldError,
  GlassGroup,
  GlassInputRow,
  GlassSectionHeader,
  GlassToggleRow,
  Picker,
  PickerTrigger,
  Prompt,
  ShortcutKeys,
  type DevicePickerDevice,
} from "@exp/ui"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { agentCollection, deviceCollection, teamCollection } from "@/lib/collections"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"

export type VoiceSettings = Awaited<ReturnType<typeof trpc.voice.get.query>>
type VoiceChange = Partial<VoiceSettings>

/** A voice device candidate: the synced device row of the member. */
export interface VoiceDeviceRow {
  deviceId: string
  name: string
  kind: string
  icon?: string | null
  /** #NEW-57: the device doctor's `desktopAudio` (absent = `none`, `[]`). */
  desktopAudio?: { capability: string; apps: readonly AudioApp[] } | null
  /** S-3: the device is online (its heartbeat is fresh; absent = online). */
  online?: boolean
  /** S-3: its doctor row `wake` (group voice) detail; absent = not reported. */
  wakeDetail?: string | null
}

/** S-3: the Wake word row's detail on a client that is not the voice
 *  device: no agent and offline first (`wakeDetail`), then the device's own
 *  words (its doctor row `wake`), else `starting`. */
export function wakeRowDetail(
  settings: { wakeWord: boolean; agentId: string | null },
  device: VoiceDeviceRow | undefined,
  deviceName: string
): string | null {
  const context = {
    wakeWord: settings.wakeWord,
    agent: settings.agentId !== null,
    online: device?.online ?? false,
    device: deviceName,
    state: null,
  }
  const base = wakeDetail(context)
  if (base === null || !context.agent || !context.online) return base
  return device?.wakeDetail || base
}

/** #NEW-57: an unknown or absent capability reads as `none`. */
function capabilityOf(device: VoiceDeviceRow | undefined): DesktopAudioCapability {
  const capability = device?.desktopAudio?.capability
  return capability === `per_app` || capability === `system` ? capability : `none`
}

const DESKTOP_AUDIO_LABELS = new Map(DESKTOP_AUDIO_VALUES.map((option) => [option.value, option.label] as const))

/** The agent picker's row: `VoiceAgentLike` (the option builder's input). */
export type VoiceAgentRow = VoiceAgentLike

const WAKE_PHRASE_DEBOUNCE_MS = 800

/** The one-line text rows: typed locally, saved debounced or on blur. */
type TextField = `wakePhrase` | `voiceFile` | `voiceTranscript`
const TEXT_FIELDS: readonly TextField[] = [`wakePhrase`, `voiceFile`, `voiceTranscript`]
const TEXT_LIMITS: Record<TextField, number> = {
  wakePhrase: VOICE_LIMITS.wakePhrase,
  voiceFile: VOICE_LIMITS.voiceFile,
  voiceTranscript: VOICE_LIMITS.voiceTranscript,
}
const TEXT_FIELD_NAMES: Record<TextField, string> = {
  wakePhrase: `the wake phrase`,
  voiceFile: `the voice file`,
  voiceTranscript: `the transcript`,
}
const textOf = (settings: VoiceSettings): Record<TextField, string> => ({
  wakePhrase: settings.wakePhrase,
  voiceFile: settings.voiceFile,
  voiceTranscript: settings.voiceTranscript,
})

export function VoiceSettingsCard({
  settings: initial,
  devices,
  agents,
  consentOpen: initialConsentOpen = false,
  assistantId,
}: {
  settings: VoiceSettings
  devices: readonly VoiceDeviceRow[]
  agents: readonly VoiceAgentRow[]
  /** The styleguide's open-dialog state (meeting-consent). */
  consentOpen?: boolean
  /** S-3 (agent-settings.json `assistant`): the block sits on THIS
   *  Assistant's page: no Voice device row (the device is Runs on's) and no
   *  Agent row (the agent is this Assistant); a change made here moves the
   *  voice onto it when another of the member's Assistants held it. */
  assistantId?: string
}) {
  const [settings, setSettings] = useState<VoiceSettings>(initial)
  // #NEW-57: THE consent dialog; turning Meeting mode on only opens it.
  const [consentOpen, setConsentOpen] = useState(initialConsentOpen)
  const [text, setText] = useState<Record<TextField, string>>(() => textOf(initial))
  // #NEW-56: a too-short wake phrase is invalid input: never sent, its line
  // shows under the field once it is left, until the next edit.
  const [wakePhraseInvalid, setWakePhraseInvalid] = useState<string | null>(null)
  // Last write wins: only the newest request may settle or revert the state.
  const seqRef = useRef(0)
  const textTimersRef = useRef<Partial<Record<TextField, ReturnType<typeof setTimeout>>>>({})
  const sentTextRef = useRef<Record<TextField, string>>(textOf(initial))

  const deviceItems = useMemo(() => {
    const byId = new Map(devices.map((device) => [device.deviceId, device] as const))
    const pickable: DevicePickerDevice[] = voiceDeviceOptions(
      devices.map((device) => ({ deviceId: device.deviceId, name: device.name, kind: device.kind }))
    ).flatMap((option) => {
      if (option.id === null) return []
      const device = byId.get(option.id)
      return [{ id: option.id, name: option.label, kind: device?.kind, icon: device?.icon }]
    })
    return devicePickerItems(pickable)
  }, [devices])

  const agentItems = useMemo(
    () =>
      agentOptions([...agents]).flatMap((option) =>
        option.id === null ? [] : [{ value: option.id, label: option.label }]
      ),
    [agents]
  )

  // #NEW-71: never the raw device id; a row not (yet) synced reads `…`.
  const deviceName = devices.find((device) => device.deviceId === settings.voiceDeviceId)?.name ?? VOICE_COPY.ellipsis
  const agentName = agents.find((agent) => agent.id === settings.agentId)?.name ?? VOICE_COPY.agent
  const switchesLive = settings.voiceDeviceId !== null && settings.agentId !== null
  // #NEW-57: the voice device's capture capability and its playing apps.
  const voiceDevice = devices.find((device) => device.deviceId === settings.voiceDeviceId)
  const capability = capabilityOf(voiceDevice)
  const liveApps = voiceDevice?.desktopAudio?.apps ?? []
  const meetingOn = switchesLive && settings.meetingStartedAt !== null
  const whoText = who(settings.desktopAudio, settings.desktopAudioApps, capability)
  const desktopAudioItems = desktopAudioOptions(capability).map((value) => ({
    value,
    label: DESKTOP_AUDIO_LABELS.get(value) ?? value,
  }))
  // The Apps row: only while the stored value picks or leaves out apps AND
  // the device can honour it (per_app offers include/exclude).
  const showApps =
    (settings.desktopAudio === `include` || settings.desktopAudio === `exclude`) &&
    desktopAudioOptions(capability).includes(settings.desktopAudio)
  const appItems = appOptions(liveApps, settings.desktopAudioApps)
  const pickedApps = settings.desktopAudioApps.map((app) => app.name).join(`, `)
  // #NEW-73: every language carries its GPU note (#NEW-74), in the row and in the menu.
  const languageItems = VOICE_LANGUAGES.map((language) => ({
    value: language.value,
    label: languageLabel(language.value, settings.voiceDeviceId === null ? null : deviceName),
  }))

  // The text rows settle through their own saves, never through `apply`.
  const keepText = (next: VoiceSettings, current: VoiceSettings): VoiceSettings => ({
    ...next,
    ...textOf(current),
  })

  const apply = (change: VoiceChange) => {
    const before = settings
    const next = clearOnNone(settings, change)
    setSettings(next)
    const seq = ++seqRef.current
    const sent: VoiceChange =
      assistantId && settings.agentId !== assistantId ? { ...change, agentId: assistantId } : change
    void trpc.voice.update
      .mutate(sent, SKIP_ERROR_TOAST)
      .then((stored) => {
        if (seq === seqRef.current) setSettings((current) => keepText(stored, current))
      })
      .catch((error: unknown) => {
        if (seq === seqRef.current) setSettings((current) => keepText(before, current))
        failedAction(`save`, `the voice settings`, error)
      })
  }

  // #NEW-57: Start / Stop go through their own procedures (never `update`).
  const meeting = (start: boolean) => {
    const before = settings
    setSettings((current) => ({
      ...current,
      meetingStartedAt: start ? (current.meetingStartedAt ?? new Date().toISOString()) : null,
    }))
    const seq = ++seqRef.current
    const call = start
      ? trpc.voice.startMeeting.mutate({ consent: true }, SKIP_ERROR_TOAST)
      : trpc.voice.stopMeeting.mutate(undefined, SKIP_ERROR_TOAST)
    return call
      .then((stored) => {
        if (seq === seqRef.current) setSettings((current) => keepText(stored, current))
      })
      .catch((error: unknown) => {
        if (seq === seqRef.current) setSettings((current) => keepText(before, current))
        failedAction(start ? `start` : `stop`, `meeting mode`, error)
      })
  }

  const pickApps = (ids: string[]) => {
    const byId = new Map(appItems.map((option) => [option.value, option.label] as const))
    apply({ desktopAudioApps: ids.flatMap((id) => (byId.has(id) ? [{ id, name: byId.get(id)! }] : [])) })
  }

  const saveText = (field: TextField, value: string, left = false) => {
    const timer = textTimersRef.current[field]
    if (timer) clearTimeout(timer)
    delete textTimersRef.current[field]
    const trimmed = value.trim()
    const invalid = field === `wakePhrase` ? wakePhraseError(trimmed) : null
    if (invalid) {
      if (left) setWakePhraseInvalid(invalid)
      return
    }
    if (trimmed === sentTextRef.current[field]) return
    const previous = sentTextRef.current[field]
    sentTextRef.current[field] = trimmed
    setSettings((current) => ({ ...current, [field]: trimmed }))
    void trpc.voice.update.mutate({ [field]: trimmed } as Partial<Record<TextField, string>>, SKIP_ERROR_TOAST).catch((error: unknown) => {
      sentTextRef.current[field] = previous
      setSettings((current) => ({ ...current, [field]: previous }))
      setText((current) => ({ ...current, [field]: previous }))
      failedAction(`save`, TEXT_FIELD_NAMES[field], error)
    })
  }

  const editText = (field: TextField, raw: string) => {
    // #NEW-75: THE input rule ×4 (control characters dropped, cut to the
    // server's UTF-16 limit), as you type.
    const value = voiceTextInput(raw, TEXT_LIMITS[field])
    setText((current) => ({ ...current, [field]: value }))
    if (field === `wakePhrase`) setWakePhraseInvalid(null)
    const timer = textTimersRef.current[field]
    if (timer) clearTimeout(timer)
    textTimersRef.current[field] = setTimeout(() => saveText(field, value), WAKE_PHRASE_DEBOUNCE_MS)
  }

  useEffect(
    () => () => {
      for (const field of TEXT_FIELDS) {
        const timer = textTimersRef.current[field]
        if (timer) clearTimeout(timer)
      }
    },
    []
  )

  return (
    <div className="space-y-3" data-testid={VOICE_TEST_IDS.page}>
      <GlassGroup>
        {assistantId === undefined && (
          <>
            <Picker
              mode="single"
              triggerVariant="row"
              width="md"
              mobileTitle={VOICE_COPY.voiceDevice}
              data-testid={VOICE_TEST_IDS.voiceDevice}
              value={settings.voiceDeviceId}
              items={deviceItems}
              noneLabel={VOICE_COPY.voiceDeviceNone}
              onNone={() => apply({ voiceDeviceId: null })}
              onChange={(voiceDeviceId) => apply({ voiceDeviceId })}
            />
            <Picker
              mode="single"
              triggerVariant="row"
              width="md"
              mobileTitle={VOICE_COPY.agent}
              data-testid={VOICE_TEST_IDS.agent}
              value={settings.agentId}
              items={agentItems}
              noneLabel={VOICE_COPY.agentNone}
              onNone={() => apply({ agentId: null })}
              onChange={(agentId) => apply({ agentId })}
            />
          </>
        )}
        <Picker
          mode="single"
          triggerVariant="row"
          width="md"
          mobileTitle={VOICE_COPY.language}
          data-testid={VOICE_TEST_IDS.language}
          value={settings.language}
          items={languageItems}
          onChange={(language) => apply({ language })}
        />
        {settings.voiceDeviceId !== null && (
          <>
            <GlassInputRow
              id={VOICE_TEST_IDS.voiceFile}
              data-testid={VOICE_TEST_IDS.voiceFile}
              label={VOICE_COPY.voiceFile}
              placeholder={VOICE_COPY.voiceFilePlaceholder}
              value={text.voiceFile}
              onChange={(event) => editText(`voiceFile`, event.target.value)}
              onBlur={() => saveText(`voiceFile`, text.voiceFile)}
            />
            <GlassInputRow
              id={VOICE_TEST_IDS.voiceTranscript}
              data-testid={VOICE_TEST_IDS.voiceTranscript}
              label={VOICE_COPY.voiceTranscript}
              placeholder={VOICE_COPY.voiceTranscriptPlaceholder}
              value={text.voiceTranscript}
              onChange={(event) => editText(`voiceTranscript`, event.target.value)}
              onBlur={() => saveText(`voiceTranscript`, text.voiceTranscript)}
            />
          </>
        )}
      </GlassGroup>
      {settings.voiceDeviceId !== null && (
        <>
          <GlassSectionHeader label={trayHeader(deviceName)} />
          <GlassGroup>
            {/* S-3: read-only here; keys are recorded on the voice device. */}
            <div
              data-slot="glass-value-row"
              data-testid={VOICE_TEST_IDS.shortcut}
              className="flex items-center gap-3 px-4 py-3"
            >
              <span className="flex-1 text-sm font-normal">{VOICE_COPY.pushToTalk}</span>
              <ShortcutKeys value={settings.pttShortcut} />
            </div>
            <GlassToggleRow
              id={VOICE_TEST_IDS.wakeWord}
              label={VOICE_COPY.wakeWord}
              description={switchesLive ? (wakeRowDetail(settings, voiceDevice, deviceName) ?? undefined) : undefined}
              checked={switchesLive && settings.wakeWord}
              onCheckedChange={(wakeWord) => apply({ wakeWord })}
              disabled={!switchesLive}
            />
            <GlassInputRow
              id={VOICE_TEST_IDS.wakePhrase}
              data-testid={VOICE_TEST_IDS.wakePhrase}
              label={VOICE_COPY.wakePhrase}
              placeholder={wakePhrasePlaceholder(agentName)}
              aria-invalid={wakePhraseInvalid !== null || undefined}
              aria-describedby={wakePhraseInvalid === null ? undefined : `${VOICE_TEST_IDS.wakePhrase}-error`}
              value={text.wakePhrase}
              onChange={(event) => editText(`wakePhrase`, event.target.value)}
              onBlur={() => saveText(`wakePhrase`, text.wakePhrase, true)}
            />
          </GlassGroup>
          <FieldError id={`${VOICE_TEST_IDS.wakePhrase}-error`} className="px-1">
            {wakePhraseInvalid}
          </FieldError>
          {/* #NEW-57: the meeting rows, their own group under the tray group. */}
          <GlassGroup>
            <GlassToggleRow
              id={MEETING_TEST_IDS.meeting}
              label={MEETING_COPY.meeting}
              description={meetingOn ? whoText : undefined}
              checked={meetingOn}
              onCheckedChange={(on) => {
                if (on) setConsentOpen(true)
                else void meeting(false)
              }}
              disabled={!switchesLive}
            />
            <Picker
              mode="single"
              triggerVariant="row"
              width="md"
              mobileTitle={MEETING_COPY.desktopAudio}
              triggerLabel={DESKTOP_AUDIO_LABELS.get(settings.desktopAudio)}
              data-testid={MEETING_TEST_IDS.desktopAudio}
              value={settings.desktopAudio}
              items={desktopAudioItems}
              onChange={(desktopAudio) => apply({ desktopAudio: desktopAudio as DesktopAudio })}
            />
            {showApps && (
              <Picker
                mode="multi"
                width="md"
                mobileTitle={MEETING_COPY.apps}
                data-testid={MEETING_TEST_IDS.apps}
                value={settings.desktopAudioApps.map((app) => app.id)}
                items={appItems}
                max={MEETING_LIMITS.apps}
                onChange={pickApps}
                trigger={
                  <PickerTrigger
                    variant="row"
                    label={MEETING_COPY.apps}
                    value={pickedApps || MEETING_COPY.appsNone}
                  />
                }
              />
            )}
          </GlassGroup>
          <Prompt
            open={consentOpen}
            onOpenChange={setConsentOpen}
            title={MEETING_COPY.consentTitle}
            body={consentBody(whoText, agentName, contract.meeting.retentionDays)}
            actions={[
              { label: MEETING_COPY.cancel, role: `cancel` },
              {
                label: MEETING_COPY.start,
                role: `primary`,
                testId: MEETING_TEST_IDS.consentStart,
                onSelect: async () => {
                  await meeting(true)
                  setConsentOpen(false)
                },
              },
            ]}
          />
        </>
      )}
    </div>
  )
}

/** The page body: the card over the synced devices of the member and the
 *  agents of the member's teams. */
export function VoiceSettingsSection({
  settings,
  assistantId,
}: {
  settings: VoiceSettings
  /** S-3: the Assistant page's Voice section (VoiceSettingsCard). */
  assistantId?: string
}) {
  const { data: session } = useSession()
  const userId = session?.user?.id
  const { data: deviceRows } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  const { data: agentRows } = useLiveQuery((query) => query.from({ a: agentCollection }), [])
  const { data: teamRows } = useLiveQuery((query) => query.from({ t: teamCollection }), [])
  const now = useNow(15_000)

  const devices = useMemo<VoiceDeviceRow[]>(
    () =>
      (deviceRows ?? [])
        .filter((device) => device.userId === userId)
        .map((device) => ({
          deviceId: device.deviceId,
          name: device.label,
          kind: device.kind,
          icon: device.icon,
          desktopAudio: device.doctor?.desktopAudio ?? null,
          online: deviceRowIsOnline(device.lastSeenAt, now),
          wakeDetail:
            device.doctor?.items.find((item) => item.key === `wake` && item.group === `voice`)?.detail ?? null,
        })),
    [deviceRows, userId, now]
  )
  const agents = useMemo<VoiceAgentRow[]>(() => {
    const teamNames = new Map((teamRows ?? []).map((team) => [team.id, team.name] as const))
    return (agentRows ?? []).map((agent) => ({
      id: agent.id,
      name: agent.name,
      team: teamNames.get(agent.teamId) ?? ``,
    }))
  }, [agentRows, teamRows])

  return <VoiceSettingsCard settings={settings} devices={devices} agents={agents} assistantId={assistantId} />
}
