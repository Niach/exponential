import { RunStatusRow, type RunMarkState } from "@exp/ui"
import type { CodingSession } from "@/db/schema"
import { AgentSessionView } from "@/components/agent-session"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { runHasEnded } from "@/lib/past-runs"
import type { SessionIdentity } from "@/lib/session-identity"
import {
  runRowCaption,
  runRowState as resolveRunRowState,
  type RunRowState,
} from "@/lib/work-faces"
import { useSessionDevice } from "@/hooks/use-session-device"
import { useNow } from "@/hooks/use-now"

// EXP-1284: a session block's BODY on the thread page — the session's
// EXISTING Run-face content. For the OWNER that is the Run face itself,
// embedded (`AgentSessionView embedded`): the status row over its turns /
// thread (the prompt bubble, narration, published results), the pending
// plan or question card answered IN its card, and Show work swapping in the
// transcript in place. A teammate sees the status row only (EXP-312: a live
// session's words stay with its owner; no ticket is ever minted for them).

export function ThreadRunBody({
  session,
  currentUserId,
  identity,
  prState,
  initialShowWork = false,
  className,
}: {
  session: CodingSession
  currentUserId: string
  /** The session's identity (its subject names the block when folded). */
  identity: SessionIdentity
  /** The PR state the display reads (the issue's, else the session's own). */
  prState: string | null
  /** A folded block's Show work opens straight on the transcript. */
  initialShowWork?: boolean
  className?: string
}) {
  if (session.userId !== currentUserId) {
    return (
      <TeammateRunBody session={session} prState={prState} className={className} />
    )
  }
  return (
    <OwnRunBody
      session={session}
      identity={identity}
      initialShowWork={initialShowWork}
      className={className}
    />
  )
}

function statusRowFor(
  session: CodingSession,
  prState: string | null,
  extra: { paused: boolean; working: boolean }
): RunRowState {
  return resolveRunRowState({
    paused: extra.paused,
    ended: runHasEnded(session),
    awaitingInput: false,
    working: extra.working,
    display: sessionDisplayState(session, prState),
  })
}

function markOf(state: RunRowState): RunMarkState | undefined {
  return state === `paused` ? undefined : state
}

function TeammateRunBody({
  session,
  prState,
  className,
}: {
  session: CodingSession
  prState: string | null
  className?: string
}) {
  const device = useSessionDevice(session)
  const state = statusRowFor(session, prState, {
    paused: device.online === false && !runHasEnded(session),
    working: false,
  })
  const now = useNow(state === `working` ? 1000 : 30_000)
  const caption = runRowCaption({
    state,
    device: device.label || session.deviceLabel || `Desktop`,
    startedAt: session.startedAt,
    endedAt: session.endedAt ?? session.updatedAt,
    now,
  })
  return (
    <div className={className} data-testid="thread-run-body" data-owner="false">
      <RunStatusRow
        agent={session.agent}
        markState={markOf(state)}
        caption={caption.text}
        tone={caption.tone}
      />
    </div>
  )
}

/** The owner's block: the Run face's body itself (`AgentSessionView`
 *  embedded) — status row, turns / thread, the pending card answered in
 *  place, Show work swapping in the transcript in place. */
function OwnRunBody({
  session,
  identity,
  initialShowWork,
  className,
}: {
  session: CodingSession
  identity: SessionIdentity
  initialShowWork: boolean
  className?: string
}) {
  return (
    <div className={className} data-testid="thread-run-body" data-owner="true">
      <AgentSessionView
        embedded
        initialShowWork={initialShowWork}
        session={session}
        currentUserId={session.userId}
        identity={identity}
        face="run"
        onFace={noop}
        onBack={noop}
      />
    </div>
  )
}

function noop(): void {}
