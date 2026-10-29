import { and, eq, isNull } from "drizzle-orm"
import { MEETING_COPY, stopNote, type MeetingStopReason } from "@exp/etalk/meeting"
import { agents, devices, sessions } from "@/db/schema"
import { insertMessage } from "@/lib/sessions/messages"
import type { Db } from "@/lib/sessions/tx"

// NEW-57 (meeting.json, `copy.sharedRefused`): a meeting transcript never
// lands in a team-visible conversation or on another member's machine. The
// voice agent's device must be the caller's OWN row and not shared with any
// team (a conversation there is created `shared`, and a shared box may be
// hosted for teammates), and the caller's conversation with it, if it
// exists, must not be `shared` (stamped at creation; a later unshare does
// not clear it). Both `voice.startMeeting` and `sessions.send {meeting}`
// refuse otherwise.

export const MEETING_SHARED_REFUSED = MEETING_COPY.sharedRefused

/** May `userId` send private material (meeting transcripts, screen memory:
 *  screen-memory.json TOOL) to `agentId`? */
export async function voiceAgentIsPrivate(exec: Db, userId: string, agentId: string): Promise<boolean> {
  const [agent] = await exec
    .select({ deviceId: agents.deviceId })
    .from(agents)
    .where(eq(agents.id, agentId))
    .limit(1)
  if (!agent) return false
  const [device] = await exec
    .select({ sharedTeamIds: devices.sharedTeamIds })
    .from(devices)
    .where(and(eq(devices.userId, userId), eq(devices.deviceId, agent.deviceId)))
    .limit(1)
  // A machine shared with ANY team may host teammates' runs (meeting.json
  // SHARING: a shared machine never holds a meeting).
  if (!device || (device.sharedTeamIds ?? []).length > 0) return false
  const [conversation] = await exec
    .select({ shared: sessions.shared })
    .from(sessions)
    .where(and(eq(sessions.agentId, agentId), eq(sessions.userId, userId), isNull(sessions.parentId)))
    .limit(1)
  return conversation?.shared !== true
}

/** The meeting callers' name for [`voiceAgentIsPrivate`]. */
export const meetingAgentIsPrivate = voiceAgentIsPrivate

/** FEED-163 (meeting.json NOTE): an automatic stop's ONE system row in the
 *  member's conversation with the voice agent: plain text, pre-consumed so
 *  it NEVER wakes the agent (no turn starts). No conversation yet = no note.
 *  The caller posts it only after its conditional clear actually cleared the
 *  meeting, so it lands once per meeting. Returns the message id, or null. */
export async function postMeetingStopNote(
  exec: Db,
  userId: string,
  agentId: string,
  reason: MeetingStopReason,
  hours: number
): Promise<string | null> {
  const body = stopNote(reason, hours)
  if (!body) return null
  const [conversation] = await exec
    .select({ id: sessions.id, rootId: sessions.rootId, teamId: sessions.teamId, shared: sessions.shared, userId: sessions.userId })
    .from(sessions)
    // Private only (a shared conversation never carries meeting rows, as
    // `meetingAllowed` refuses it); the eldest, so a duplicate never moves it.
    .where(
      and(eq(sessions.agentId, agentId), eq(sessions.userId, userId), isNull(sessions.parentId), eq(sessions.shared, false))
    )
    .orderBy(sessions.createdAt, sessions.id)
    .limit(1)
  if (!conversation) return null
  return exec.transaction((tx) =>
    insertMessage(tx, conversation, { authorKind: `system`, body, consumedAt: new Date() })
  )
}
