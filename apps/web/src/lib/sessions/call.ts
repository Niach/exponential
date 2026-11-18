import { and, desc, eq, isNotNull } from "drizzle-orm"
import type { SessionVoiceCall } from "@exp/db-schema/domain"
import {
  VOICE_CALL_SESSION,
  callDuration,
  callSessionPick,
  callSessionText,
  callTimer,
} from "@exp/etalk/voice-call"
import { db } from "@/db/connection"
import { sessions, type Session } from "@/db/schema"
import { findConversation, loadAgentForUser } from "@/lib/agents/conversation"
import { markEnded, relayKillSessionsBestEffort } from "@/lib/sessions/lifecycle"
import { postSystemCard } from "@/lib/sessions/messages"
import { sendSession } from "@/lib/sessions/send"

// #EXP-1961 (etalk voice-call.json `callSession`): a call runs in its own
// CALL session, a child of the chat (the caller's conversation with the
// agent). The host asks for it while it rings (`voice.callSession`), says
// its first reply as the opening line, sends every utterance into it and
// reports the hang-up (`voice.callEnded`); its next reply is the summary,
// a waking report card in the chat (lib/sessions/bus.ts `busCallTurn`).

export interface CallSessionAnswer {
  sessionId: string
  chatId: string
  /** The same call session as a call within the resume window. */
  resumed: boolean
}

/** The chat's newest call child. */
async function newestCallChild(chatId: string): Promise<Session | null> {
  const [row] = await db
    .select()
    .from(sessions)
    .where(and(eq(sessions.parentId, chatId), isNotNull(sessions.voiceCall)))
    .orderBy(desc(sessions.createdAt))
    .limit(1)
  return row ?? null
}

/** Starts or resumes the call session of `userId`'s call to `agentId`.
 *  Null = no chat with that agent yet, or the child could not start: the
 *  host keeps the old path (utterances into the chat). */
export async function startCallSession(args: {
  userId: string
  agentId: string
  callId: string | null
  now?: Date
}): Promise<CallSessionAnswer | null> {
  const { userId, callId } = args
  const now = args.now ?? new Date()
  const agent = await loadAgentForUser(userId, args.agentId)
  const chat = await findConversation(db, agent.id, userId)
  if (!chat) return null
  const language = agent.voice?.language ?? null
  const prev = await newestCallChild(chat.id)
  const pick = callSessionPick(
    prev?.voiceCall
      ? { status: prev.status, endedAtMs: prev.voiceCall.endedAt ? Date.parse(prev.voiceCall.endedAt) : null }
      : null,
    now.getTime()
  )
  if (pick === `resume` && prev?.voiceCall) {
    const call = prev.voiceCall
    // The same running call asking again (a reconnect): nothing to say.
    const reconnect = call.endedAt === null && callId !== null && call.callId === callId
    if (!reconnect) {
      const next: SessionVoiceCall = {
        ...call,
        callId,
        segmentStartedAt: now.toISOString(),
        endedAt: null,
        summaryPending: false,
      }
      await db.update(sessions).set({ voiceCall: next }).where(eq(sessions.id, prev.id))
      await sendSession({
        userId,
        input: { sessionId: prev.id, text: callSessionText(VOICE_CALL_SESSION.resumeText, language) },
        extras: { serverComposed: true },
      })
    }
    return { sessionId: prev.id, chatId: chat.id, resumed: true }
  }
  if (pick === `replace` && prev) await endCallQuietly(prev.id)
  const mark: SessionVoiceCall = {
    callId,
    startedAt: now.toISOString(),
    segmentStartedAt: now.toISOString(),
    endedAt: null,
    durationMs: 0,
    summaryPending: false,
    readUpTo: now.toISOString(),
  }
  try {
    const started = await sendSession({
      userId,
      input: {
        parentId: chat.id,
        name: VOICE_CALL_SESSION.title.en,
        text: callSessionText(VOICE_CALL_SESSION.request, language),
      },
      extras: {
        serverComposed: true,
        within: async (tx, { sessionId }) => {
          await tx.update(sessions).set({ voiceCall: mark }).where(eq(sessions.id, sessionId))
        },
      },
    })
    return { sessionId: started.sessionId, chatId: chat.id, resumed: false }
  } catch (err) {
    console.warn(`[voice] call session for chat ${chat.id} did not start: ${err instanceof Error ? err.message : err}`)
    return null
  }
}

/** A call session past its resume window ends without a second report
 *  (its summary already reached the chat). */
async function endCallQuietly(sessionId: string): Promise<void> {
  const ended = await db.transaction(async (tx) => {
    const ids = await markEnded(tx, [sessionId])
    for (const id of ids) await postSystemCard(tx, id, { kind: `event`, event: `session_ended`, sessionId: id })
    return ids
  })
  if (ended.length > 0) await relayKillSessionsBestEffort(ended)
}

/** The hang-up: the call's segment ends and the call session is asked for
 *  its summary (its next reply, `busCallTurn`). Idempotent per segment. */
export async function endCallSession(args: { userId: string; sessionId: string; now?: Date }): Promise<boolean> {
  const now = args.now ?? new Date()
  const [row] = await db
    .select({ voiceCall: sessions.voiceCall, userId: sessions.userId, status: sessions.status })
    .from(sessions)
    .where(eq(sessions.id, args.sessionId))
    .limit(1)
  if (!row?.voiceCall || row.userId !== args.userId || row.status === `ended`) return false
  const call = row.voiceCall
  if (call.endedAt !== null) return false
  const segmentMs = Math.max(0, now.getTime() - Date.parse(call.segmentStartedAt))
  const next: SessionVoiceCall = {
    ...call,
    endedAt: now.toISOString(),
    durationMs: callDuration(call.durationMs, Date.parse(call.segmentStartedAt), now.getTime()),
    summaryPending: true,
  }
  await db.update(sessions).set({ voiceCall: next }).where(eq(sessions.id, args.sessionId))
  await sendSession({
    userId: args.userId,
    input: {
      sessionId: args.sessionId,
      text: VOICE_CALL_SESSION.endedText.replaceAll(`{minutes}`, callTimer(segmentMs)),
    },
    extras: { serverComposed: true },
  })
  return true
}
