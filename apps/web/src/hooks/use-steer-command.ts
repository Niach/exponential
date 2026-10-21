import { useCallback } from "react"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { parseSteerCommand, steerCommandsFor } from "@/lib/steer-commands"
import { trpc } from "@/lib/trpc-client"

// EXP-1323 (#EXP-1330): the web's dispatch of the steer commands that are
// NOT agent text. Every other command (`/compact`, `/clear`, the agent's own)
// rides the ordinary message as text; `/btw <question>` instead starts an
// ASIDE child of the session (`sessions.send({parentId, text, aside: true})`):
// a forked read-only run the parent never sees, shown in the Conversation as
// a plain session card titled with the question. `SessionComposer.send` runs
// `dispatch` before its own `sessions.send({sessionId})` (×4: IDE
// `steer_viewer::send_confirmed`, iOS `AgentSessionModel.sendMessage`,
// Android `AgentSessionViewModel.sendDraft`); the `/` menu lists it off the
// contract (`steerCommandsFor`, claude only).

export const BTW_COMMAND = `btw`

/** What `dispatch` did with a draft: `null` = not a command handled here
 *  (send it as a message), `sent` = the aside is in (clear the draft),
 *  `failed` = toasted `Couldn't ask the side question` (keep the draft). */
export type SteerDispatch = `sent` | `failed` | null

/** The question of a `/btw <question>` draft for `agent`'s session, or null
 *  (another command, prose, no question, an agent without `/btw`). */
export function btwQuestion(text: string, agent: string | null | undefined): string | null {
  const parsed = parseSteerCommand(text, steerCommandsFor(agent))
  if (parsed?.command.name !== BTW_COMMAND) return null
  return parsed.args || null
}

/** `dispatch(text)` → truthy when the draft was a command handled here
 *  (`sent`: the composer clears it; `failed`: it toasted, the draft stays),
 *  null when it should go out as a message. */
export function useSteerCommand(input: {
  teamId: string
  sessionId: string
  /** The session's agent CLI (`/btw` is claude-only). */
  agent: string | null | undefined
}): (text: string) => Promise<SteerDispatch> {
  const { teamId, sessionId, agent } = input
  return useCallback(
    async (text: string) => {
      const question = btwQuestion(text, agent)
      if (question === null) return null
      try {
        await trpc.sessions.send.mutate(
          { teamId, parentId: sessionId, text: question },
          SKIP_ERROR_TOAST
        )
        return `sent`
      } catch (error) {
        failedAction(`ask`, `the side question`, error)
        return `failed`
      }
    },
    [teamId, sessionId, agent]
  )
}
