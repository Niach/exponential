// NEW-70 (voice V6, `fixtures/voice-call.json`): caller mode on web. THE
// telephone in the agent chat's header (left of `…`, the header's ghost icon
// button, icon concept `voice-call`), disabled with the reason as its
// tooltip (`callAvailability` over the member's voice settings and the voice
// device's synced row); and while a call runs, THE call card
// (`VoiceCallCard`, the `voice-call` entry) centred over the chat. The call
// streams to the voice device (lib/voice-call/call.ts); leaving the chat
// hangs up.
import { useEffect, useMemo, useState, useSyncExternalStore } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  callView,
  VOICE_CALL_COPY,
  VOICE_CALL_ICONS,
  VOICE_CALL_TEST_IDS,
  type CallAvailability,
} from "@exp/domain-contract/voice-call"
import { Button, cn, conceptIcon, IconTooltip, toast, VoiceCallCard } from "@exp/ui"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { deviceCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { openCallAudio } from "@/lib/voice-call/audio"
import { webCallAvailability, type CallSettingsLike } from "@/lib/voice-call/availability"
import { VoiceCall, type CallDeps } from "@/lib/voice-call/call"

const CallIcon = conceptIcon(VOICE_CALL_ICONS.call as Parameters<typeof conceptIcon>[0])

const WEB_CALL_DEPS: CallDeps = {
  startCall: (agentId) => trpc.voice.startCall.mutate({ agentId, platform: `web` }, SKIP_ERROR_TOAST),
  openSocket: (url) => new WebSocket(url),
  openAudio: openCallAudio,
}

export interface VoiceCallAvailability extends CallAvailability {
  /** The voice device's name (the toasts' `{device}`). */
  device: string
}

/** The telephone's availability: the member's voice settings (re-read when
 *  the tab comes back, Settings → Voice may have changed them) and the
 *  voice device's synced row, its freshness on a 15 s beat. */
export function useVoiceCallAvailability(): VoiceCallAvailability {
  const { data: auth } = useSession()
  const userId = auth?.user?.id ?? null
  const [settings, setSettings] = useState<CallSettingsLike | null>(null)
  useEffect(() => {
    let live = true
    const load = () =>
      void trpc.voice.get
        .query()
        .then((next) => live && setSettings(next))
        .catch(() => {})
    load()
    const onVisible = () => document.visibilityState === `visible` && load()
    document.addEventListener(`visibilitychange`, onVisible)
    return () => {
      live = false
      document.removeEventListener(`visibilitychange`, onVisible)
    }
  }, [])
  const voiceDeviceId = settings?.voiceDeviceId ?? null
  const { data: rows } = useLiveQuery(
    (query) =>
      voiceDeviceId === null
        ? undefined
        : query.from({ d: deviceCollection }).where(({ d }) => eq(d.deviceId, voiceDeviceId)),
    [voiceDeviceId]
  )
  const device = (rows ?? []).find((row) => row.userId === userId) ?? null
  const now = useNow(15_000)
  return useMemo(
    () => ({ ...webCallAvailability(settings, device, now), device: device?.label ?? `` }),
    [settings, device, now]
  )
}

/** One call per chat; the chat going away (or another agent) hangs up. */
export function useVoiceCall(agentId: string) {
  const call = useMemo(() => {
    const next = new VoiceCall(WEB_CALL_DEPS)
    next.onToast = (message) => void toast.error(message)
    return next
  }, [agentId])
  useEffect(() => () => call.end(), [call])
  const snapshot = useSyncExternalStore(call.subscribe, call.getSnapshot, call.getSnapshot)
  return { call, snapshot }
}

/** THE telephone: the header's ghost icon button, its tooltip the call or
 *  the reason it is off. */
export function CallButton({
  availability,
  agentName,
  active,
  onCall,
  className,
}: {
  availability: CallAvailability
  agentName: string
  active: boolean
  onCall: () => void
  className?: string
}) {
  const label = VOICE_CALL_COPY.call.replace(`{agent}`, agentName)
  const disabled = !availability.enabled || active
  return (
    <IconTooltip label={availability.reason ?? label}>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        aria-label={label}
        disabled={disabled}
        data-testid={VOICE_CALL_TEST_IDS.call}
        data-reason={availability.reason ?? undefined}
        className={cn(`disabled:opacity-40`, className)}
        onClick={onCall}
      >
        <CallIcon />
      </Button>
    </IconTooltip>
  )
}

/** The centred call card while a call runs: the card fill on the opaque
 *  base (`bg-glass-card-opaque`, the floating composer's), so the chat
 *  scrolling behind never shows through. */
export function VoiceCallOverlay({ call, snapshot }: ReturnType<typeof useVoiceCall>) {
  const view = callView(snapshot.state)
  if (!view) return null
  return (
    <div className="pointer-events-none absolute inset-0 z-40 flex items-center justify-center p-4">
      <VoiceCallCard
        className="pointer-events-auto bg-glass-card-opaque"
        activity={view.activity}
        level={snapshot.level}
        state={view.state}
        line={view.line}
        mute={view.mute}
        muted={snapshot.state.muted}
        endLabel={VOICE_CALL_COPY.endCall}
        onEnd={() => call.end()}
        onMute={() => call.toggleMute()}
        data-testid={VOICE_CALL_TEST_IDS.surface}
        endTestId={VOICE_CALL_TEST_IDS.end}
        muteTestId={VOICE_CALL_TEST_IDS.mute}
      />
    </div>
  )
}
