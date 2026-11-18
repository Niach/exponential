// NEW-70 (voice V6, `fixtures/voice-call.json`): caller mode on web. THE
// telephone in the agent chat's header (left of `…`, the header's ghost icon
// button, icon concept `voice-call`), disabled with the reason as its
// tooltip (EXP-1857: `callTarget` over the agent's device, the member's
// synced desktops and their voice settings); and while a call runs, THE call surface
// (`VoiceCallSurface`, the `voice-call` entry; S-3: phone-call style, a
// full-height overlay over the chat, `callView(state, 'web')`). The call
// streams to the target desktop (lib/voice-call/call.ts), ringing first; leaving
// the agent page (or closing the tab) hangs up. FEED-272: Chat / Voice tabs
// on top of the surface; Chat docks the call as the pill's row over the
// chat, which stays readable and writable underneath. Only the surface reads
// the microphone's level and ticks the timer, so neither re-renders the chat.
import { useEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  callTimer,
  callView,
  VOICE_CALL_COPY,
  VOICE_CALL_ICONS,
  VOICE_CALL_SEAL,
  VOICE_CALL_TABS,
  VOICE_CALL_TEST_IDS,
  type CallAvailability,
  type CallTab,
  type CallTarget,
} from "@exp/etalk/voice-call"
import { Button, cn, conceptIcon, CallTabs, IconTooltip, toast, VoiceCallPill, VoiceCallSurface } from "@exp/ui"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { deviceCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { SEAL_HOLDER_WAIT_MS } from "@exp/domain-contract/sealed-content"
import { awaitKeyHolder, currentKeyHolder } from "@/lib/enet/current-key-holder"
import { openCallAudio, playRingTone } from "@/lib/voice-call/audio"
import type { CallKeySource } from "@/lib/voice-call/seal"
import { webCallTarget, type CallSettingsLike } from "@/lib/voice-call/availability"
import { VoiceCall, type CallDeps } from "@/lib/voice-call/call"

const CallIcon = conceptIcon(VOICE_CALL_ICONS.call as Parameters<typeof conceptIcon>[0])

/** #EXP-1941: the chat's key of a sealed call, from this tab's key holder. */
const WEB_CALL_KEYS: CallKeySource = {
  // A holder that is still starting (the tab is finding its install) is
  // waited for, as the chat's writers do; none in time = no claim. The wait
  // is spent only when a call CAN be sealed: one of the synced desktops or
  // servers advertises the cap. Until a host does, a tab without a holder
  // dials at once, as it always did.
  ready: async () => {
    if (currentKeyHolder()) return true
    const sealable = [...deviceCollection.values()].some((row) => (row.caps ?? []).includes(VOICE_CALL_SEAL.cap))
    return sealable ? (await awaitKeyHolder(SEAL_HOLDER_WAIT_MS)) !== undefined : false
  },
  target: (agentId) => trpc.sessions.sendTarget.query({ agentId }, SKIP_ERROR_TOAST),
  secret: async (rootId, epoch) => {
    const holder = currentKeyHolder()
    if (!holder) return null
    // Not throttled: a call waits on this pull, and it nudges the devices that hold the key.
    if (!holder.keysFor(rootId).has(epoch)) await holder.refresh([rootId])
    return holder.keysFor(rootId).get(epoch) ?? null
  },
  highestEpoch: (rootId) => currentKeyHolder()?.current(rootId)?.epoch ?? null,
}

const WEB_CALL_DEPS: CallDeps = {
  startCall: (agentId, seal) => trpc.voice.startCall.mutate({ agentId, platform: `web`, seal }, SKIP_ERROR_TOAST),
  keys: WEB_CALL_KEYS,
  openSocket: (url) => new WebSocket(url),
  openAudio: openCallAudio,
  ring: playRingTone,
}

export interface VoiceCallAvailability extends CallTarget {
  /** The target desktop's name (the toasts' `{device}`). */
  device: string
  /** The member's voice language (the greeting the caption shows). */
  language: string | undefined
}

/** The telephone's availability for an agent running on `agentDevice`:
 *  the member's voice settings (re-read when the tab comes back, a meeting
 *  may have started) and their synced desktop rows, freshness on a 15 s
 *  beat (EXP-1857: `callTarget`, the agent's desktop or the fallback). */
export function useVoiceCallAvailability(agentDevice: string | null, language?: string): VoiceCallAvailability {
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
  const { data: rows } = useLiveQuery(
    (query) =>
      userId === null
        ? undefined
        : query.from({ d: deviceCollection }).where(({ d }) => eq(d.userId, userId)),
    [userId]
  )
  const now = useNow(15_000)
  return useMemo(() => {
    const target = webCallTarget(settings, agentDevice, rows ?? [], userId, now)
    return { ...target, device: target.name ?? ``, language: language ?? settings?.language }
  }, [settings, agentDevice, rows, userId, now, language])
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

/** The call while it runs (THE `VoiceCallSurface`, exactly the styleguide
 *  entry): on Voice covering the chat at full height, on Chat (FEED-272) the
 *  pill's row docked over the chat; the only reader of the microphone's
 *  level and the timer's clock. */
export function VoiceCallOverlay({
  call,
  snapshot,
  agentName,
  avatar,
  initialTab = VOICE_CALL_TABS.initial as CallTab,
}: ReturnType<typeof useVoiceCall> & { agentName: string; avatar: ReactNode; initialTab?: CallTab }) {
  const view = callView(snapshot.state, `web`)
  const level = useSyncExternalStore(call.subscribeLevel, call.getLevel, call.getLevel)
  const answeredAt = useAnsweredAt(view !== null && view.activity !== `ringing`)
  const now = useNow(1_000)
  const [tab, setTab] = useState<CallTab>(initialTab)
  // A new call opens on Voice again.
  const live = view !== null
  useEffect(() => {
    if (!live) setTab(initialTab)
  }, [live, initialTab])
  useScreenAwake(live)
  if (!view) return null
  const timer = callTimer(answeredAt === null ? 0 : now.getTime() - answeredAt)
  const shownLevel = view.muted ? 0 : level
  if (tab === `chat`) {
    return (
      <div className="flex shrink-0 flex-col gap-3 px-3 pt-3 md:px-4" data-testid={VOICE_CALL_TEST_IDS.panel} data-tab="chat">
        <CallTabs tab={tab} onTab={setTab} className="mx-auto max-w-3xl" />
        <div className="mx-auto w-full max-w-3xl">
          <VoiceCallPill
            docked
            agent={agentName}
            avatar={avatar}
            status={view.status ?? timer}
            activity={view.activity}
            level={shownLevel}
            muted={view.muted}
            buttons={view.buttons}
            end={view.end}
            onMute={() => call.toggleMute()}
            onEnd={() => call.end()}
          />
        </div>
      </div>
    )
  }
  return (
    <div className="absolute inset-0 z-40 flex flex-col bg-background" data-testid={VOICE_CALL_TEST_IDS.panel} data-tab="voice">
      <div className="shrink-0 px-3 pt-3 md:px-4">
        <CallTabs tab={tab} onTab={setTab} className="mx-auto max-w-3xl" />
      </div>
      <div className="flex min-h-0 flex-1">
        <VoiceCallSurface
          agent={agentName}
          avatar={avatar}
          view={view}
          timer={timer}
          level={shownLevel}
          onEnd={() => call.end()}
          onMute={() => call.toggleMute()}
        />
      </div>
    </div>
  )
}
