// NEW-70 (voice V6, `fixtures/voice-call.json`): caller mode on web. THE
// telephone in the agent chat's header (left of `…`, the header's ghost icon
// button, icon concept `voice-call`), disabled with the reason as its
// tooltip (`callAvailability` over the member's voice settings and the voice
// device's synced row); and while a call runs, THE call surface
// (`VoiceCallSurface`, the `voice-call` entry; S-3: phone-call style, a
// full-height overlay over the chat, `callView(state, 'web')`). The call
// streams to the voice device (lib/voice-call/call.ts), ringing first; leaving
// the chat (or the page) hangs up. Only the surface reads the microphone's
// level and ticks the timer, so neither re-renders the chat.
import { useEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  callTimer,
  callView,
  VOICE_CALL_COPY,
  VOICE_CALL_ICONS,
  VOICE_CALL_TEST_IDS,
  type CallAvailability,
} from "@exp/domain-contract/voice-call"
import { Button, cn, conceptIcon, IconTooltip, toast, VoiceCallSurface } from "@exp/ui"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { deviceCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { openCallAudio, playRingTone } from "@/lib/voice-call/audio"
import { webCallAvailability, type CallSettingsLike } from "@/lib/voice-call/availability"
import { VoiceCall, type CallDeps } from "@/lib/voice-call/call"

const CallIcon = conceptIcon(VOICE_CALL_ICONS.call as Parameters<typeof conceptIcon>[0])

const WEB_CALL_DEPS: CallDeps = {
  startCall: (agentId) => trpc.voice.startCall.mutate({ agentId, platform: `web` }, SKIP_ERROR_TOAST),
  openSocket: (url) => new WebSocket(url),
  openAudio: openCallAudio,
  ring: playRingTone,
}

export interface VoiceCallAvailability extends CallAvailability {
  /** The voice device's name (the toasts' `{device}`). */
  device: string
  /** The member's voice language (the greeting the caption shows). */
  language: string | undefined
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
    () => ({ ...webCallAvailability(settings, device, now), device: device?.label ?? ``, language: settings?.language }),
    [settings, device, now]
  )
}

/** One call per chat; the chat going away (another agent, or the page
 *  itself) hangs up. */
export function useVoiceCall(agentId: string, deps: CallDeps = WEB_CALL_DEPS) {
  const call = useMemo(() => {
    const next = new VoiceCall(deps)
    next.onToast = (message) => void toast.error(message)
    return next
  }, [agentId, deps])
  useEffect(() => {
    const hangUp = () => call.end()
    window.addEventListener(`pagehide`, hangUp)
    return () => {
      window.removeEventListener(`pagehide`, hangUp)
      hangUp()
    }
  }, [call])
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

/** When the call was answered (its first view past the ring), for the
 *  timer; null while ringing or with no call. */
function useAnsweredAt(answered: boolean): number | null {
  const [at, setAt] = useState<number | null>(null)
  useEffect(() => {
    setAt((current) => (answered ? (current ?? Date.now()) : null))
  }, [answered])
  return at
}

/** Phones and web keep the screen awake while the call surface shows. */
function useScreenAwake(active: boolean) {
  useEffect(() => {
    if (!active || typeof navigator === `undefined` || !(`wakeLock` in navigator)) return
    let lock: WakeLockSentinel | null = null
    let live = true
    const take = () =>
      void navigator.wakeLock
        .request(`screen`)
        .then((next) => {
          if (live) lock = next
          else void next.release()
        })
        .catch(() => {})
    const onVisible = () => document.visibilityState === `visible` && take()
    take()
    document.addEventListener(`visibilitychange`, onVisible)
    return () => {
      live = false
      document.removeEventListener(`visibilitychange`, onVisible)
      void lock?.release().catch(() => {})
    }
  }, [active])
}

/** The call surface while a call runs (THE `VoiceCallSurface`, exactly the
 *  styleguide entry) covering the chat at full height; the only reader of
 *  the microphone's level and the timer's clock. */
export function VoiceCallOverlay({
  call,
  snapshot,
  agentName,
  avatar,
}: ReturnType<typeof useVoiceCall> & { agentName: string; avatar: ReactNode }) {
  const view = callView(snapshot.state, `web`)
  const level = useSyncExternalStore(call.subscribeLevel, call.getLevel, call.getLevel)
  const answeredAt = useAnsweredAt(view !== null && view.activity !== `ringing`)
  const now = useNow(1_000)
  useScreenAwake(view !== null)
  if (!view) return null
  return (
    <div className="absolute inset-0 z-40 flex" data-testid={VOICE_CALL_TEST_IDS.panel}>
      <VoiceCallSurface
        agent={agentName}
        avatar={avatar}
        view={view}
        timer={callTimer(answeredAt === null ? 0 : now.getTime() - answeredAt)}
        level={view.muted ? 0 : level}
        onEnd={() => call.end()}
        onMute={() => call.toggleMute()}
      />
    </div>
  )
}
