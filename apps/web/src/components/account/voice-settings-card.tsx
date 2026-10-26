// NEW-67 (voice plan v3, V1): Settings → Voice, the member's ONE voice
// settings row (`fixtures/voice.json`). One group: the voice device (THE
// device picker's rows over `voiceDeviceOptions`, None first), the agent
// (`agentOptions`, None first) and Speak replies; while a voice device is
// picked the tray group follows under `trayHeader`. #NEW-73: Language
// (`languages`, each read through `languageLabel`) follows Speak replies,
// and while a voice device is picked the voice file + its transcript. Every switch reads off
// and is disabled while the device or the agent is None, and choosing None
// clears them (`clearOnNone`). Each change is an optimistic `voice.update`
// that reverts with the failed-action toast (feedback-states.json).
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
  VOICE_SHORTCUTS,
  VOICE_TEST_IDS,
  voiceDeviceOptions,
  wakePhrasePlaceholder,
  type VoiceAgentLike,
} from "@exp/domain-contract/voice"
import {
  devicePickerItems,
  GlassGroup,
  GlassInputRow,
  GlassSectionHeader,
  GlassToggleRow,
  Picker,
  type DevicePickerDevice,
} from "@exp/ui"
import { useSession } from "@/hooks/use-session"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { agentCollection, deviceCollection, teamCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

export type VoiceSettings = Awaited<ReturnType<typeof trpc.voice.get.query>>
type VoiceChange = Partial<VoiceSettings>

/** A voice device candidate: the synced device row of the member. */
export interface VoiceDeviceRow {
  deviceId: string
  name: string
  kind: string
  icon?: string | null
}

/** The agent picker's row: `VoiceAgentLike` (the option builder's input). */
export type VoiceAgentRow = VoiceAgentLike

const WAKE_PHRASE_DEBOUNCE_MS = 800

/** The one-line text rows: typed locally, saved debounced or on blur. */
type TextField = `wakePhrase` | `voiceFile` | `voiceTranscript`
const TEXT_FIELDS: readonly TextField[] = [`wakePhrase`, `voiceFile`, `voiceTranscript`]
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
}: {
  settings: VoiceSettings
  devices: readonly VoiceDeviceRow[]
  agents: readonly VoiceAgentRow[]
}) {
  const [settings, setSettings] = useState<VoiceSettings>(initial)
  const [text, setText] = useState<Record<TextField, string>>(() => textOf(initial))
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

  const deviceName =
    devices.find((device) => device.deviceId === settings.voiceDeviceId)?.name ??
    settings.voiceDeviceId ??
    ``
  const agentName = agents.find((agent) => agent.id === settings.agentId)?.name ?? VOICE_COPY.agent
  const switchesLive = settings.voiceDeviceId !== null && settings.agentId !== null
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
    void trpc.voice.update
      .mutate(change, SKIP_ERROR_TOAST)
      .then((stored) => {
        if (seq === seqRef.current) setSettings((current) => keepText(stored, current))
      })
      .catch((error: unknown) => {
        if (seq === seqRef.current) setSettings((current) => keepText(before, current))
        failedAction(`save`, `the voice settings`, error)
      })
  }

  const saveText = (field: TextField, value: string) => {
    const timer = textTimersRef.current[field]
    if (timer) clearTimeout(timer)
    delete textTimersRef.current[field]
    const trimmed = value.trim()
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
    // Control characters never reach the server (iPhone and Android strip them the same way).
    const value = raw.replace(/\p{Cc}/gu, ``)
    setText((current) => ({ ...current, [field]: value }))
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
        <GlassToggleRow
          id={VOICE_TEST_IDS.speakReplies}
          label={VOICE_COPY.speakReplies}
          checked={switchesLive && settings.speakReplies}
          onCheckedChange={(speakReplies) => apply({ speakReplies })}
          disabled={!switchesLive}
        />
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
              maxLength={VOICE_LIMITS.voiceFile}
              value={text.voiceFile}
              onChange={(event) => editText(`voiceFile`, event.target.value)}
              onBlur={() => saveText(`voiceFile`, text.voiceFile)}
            />
            <GlassInputRow
              id={VOICE_TEST_IDS.voiceTranscript}
              data-testid={VOICE_TEST_IDS.voiceTranscript}
              label={VOICE_COPY.voiceTranscript}
              placeholder={VOICE_COPY.voiceTranscriptPlaceholder}
              maxLength={VOICE_LIMITS.voiceTranscript}
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
            <GlassToggleRow
              id={VOICE_TEST_IDS.pushToTalk}
              label={VOICE_COPY.pushToTalk}
              checked={switchesLive && settings.pushToTalk}
              onCheckedChange={(pushToTalk) => apply({ pushToTalk })}
              disabled={!switchesLive}
            />
            <Picker
              mode="single"
              triggerVariant="row"
              width="md"
              mobileTitle={VOICE_COPY.shortcut}
              data-testid={VOICE_TEST_IDS.shortcut}
              value={settings.pttShortcut}
              items={VOICE_SHORTCUTS}
              onChange={(pttShortcut) => apply({ pttShortcut })}
            />
            <GlassToggleRow
              id={VOICE_TEST_IDS.wakeWord}
              label={VOICE_COPY.wakeWord}
              checked={switchesLive && settings.wakeWord}
              onCheckedChange={(wakeWord) => apply({ wakeWord })}
              disabled={!switchesLive}
            />
            <GlassInputRow
              id={VOICE_TEST_IDS.wakePhrase}
              data-testid={VOICE_TEST_IDS.wakePhrase}
              label={VOICE_COPY.wakePhrase}
              placeholder={wakePhrasePlaceholder(agentName)}
              maxLength={64}
              value={text.wakePhrase}
              onChange={(event) => editText(`wakePhrase`, event.target.value)}
              onBlur={() => saveText(`wakePhrase`, text.wakePhrase)}
            />
          </GlassGroup>
        </>
      )}
    </div>
  )
}

/** The page body: the card over the synced devices of the member and the
 *  agents of the member's teams. */
export function VoiceSettingsSection({ settings }: { settings: VoiceSettings }) {
  const { data: session } = useSession()
  const userId = session?.user?.id
  const { data: deviceRows } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  const { data: agentRows } = useLiveQuery((query) => query.from({ a: agentCollection }), [])
  const { data: teamRows } = useLiveQuery((query) => query.from({ t: teamCollection }), [])

  const devices = useMemo<VoiceDeviceRow[]>(
    () =>
      (deviceRows ?? [])
        .filter((device) => device.userId === userId)
        .map((device) => ({
          deviceId: device.deviceId,
          name: device.label,
          kind: device.kind,
          icon: device.icon,
        })),
    [deviceRows, userId]
  )
  const agents = useMemo<VoiceAgentRow[]>(() => {
    const teamNames = new Map((teamRows ?? []).map((team) => [team.id, team.name] as const))
    return (agentRows ?? []).map((agent) => ({
      id: agent.id,
      name: agent.name,
      team: teamNames.get(agent.teamId) ?? ``,
    }))
  }, [agentRows, teamRows])

  return <VoiceSettingsCard settings={settings} devices={devices} agents={agents} />
}
