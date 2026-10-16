import { useCallback } from "react"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { parseSteerCommand, steerCommandsFor } from "@/lib/steer-commands"
import { trpc } from "@/lib/trpc-client"

// EXP-1323 (#EXP-1330): the web's dispatch of the steer commands that are
// NOT agent text. Every other command (`/compact`, `/clear`, the agent's own)
// rides the ordinary message as text; `/btw <question>` instead starts an
// ASIDE child of the session (`sessions.send({parentId, text, aside: true})`):
// a forked read-only run the parent never sees, shown in the Conversation as
// a plain session card titled with the question.
//
// TODO(EXP-1323): the composer owner wires this into the send path (and THE
// `/` menu): `if (await dispatch(text)) clear the draft, else send as today`.

export const BTW_COMMAND = `btw`

/** The question of a `/btw <question>` draft for `agent`'s session, or null
 *  (another command, prose, no question, an agent without `/btw`). */
export function btwQuestion(text: string, agent: string | null | undefined): string | null {
  const parsed = parseSteerCommand(text, steerCommandsFor(agent))
  if (parsed?.command.name !== BTW_COMMAND) return null
  return parsed.args || null
}

/** `dispatch(text)` → true when the draft was a command handled here (the
 *  composer clears it), false when it should go out as a message. */
export function useSteerCommand(input: {
  teamId: string
  sessionId: string
  /** The session's agent CLI (`/btw` is claude-only). */
  agent: string | null | undefined
}): (text: string) => Promise<boolean> {
  const { teamId, sessionId, agent } = input
  return useCallback(
    async (text: string) => {
      const question = btwQuestion(text, agent)
      if (question === null) return false
      try {
        await trpc.sessions.send.mutate(
          { teamId, parentId: sessionId, text: question, aside: true },
          SKIP_ERROR_TOAST
        )
      } catch (error) {
        failedAction(`ask`, `the side question`, error)
      }
      return true
    },
    [teamId, sessionId, agent]
  )
}
