import { useCallback } from "react"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { parseSteerCommand, steerCommandsFor } from "@/lib/steer-commands"
import { trpc } from "@/lib/trpc-client"

// NEW-2: the web's dispatch of the steer commands that are NOT agent text.
// The builtins are contract `steerCommands` (compact, model, effort):
// `/compact` and `/model <alias>` ride the ordinary message as text (the CLI
// runs them; `config_state` reports a model switch), while `/effort <level>`
// is ours: `sessions.configure({sessionId, effort})` (the host applies it at
// the next turn). `SessionComposer.send` runs `dispatch` before its own
// `sessions.send({sessionId})`.

export const EFFORT_COMMAND = `effort`

/** What `dispatch` did with a draft: `null` = not a command handled here
 *  (send it as a message), `sent` = applied (clear the draft), `failed` =
 *  toasted `Couldn't set the effort` (keep the draft). */
export type SteerDispatch = `sent` | `failed` | null

/** The level of an `/effort <level>` draft for `agent`'s session, or null
 *  (another command, prose, no level, an agent without `/effort`). */
export function effortLevel(text: string, agent: string | null | undefined): string | null {
  const parsed = parseSteerCommand(text, steerCommandsFor(agent))
  if (parsed?.command.name !== EFFORT_COMMAND) return null
  return parsed.args.toLowerCase() || null
}

/** `dispatch(text)` → truthy when the draft was a command handled here
 *  (`sent`: the composer clears it; `failed`: it toasted, the draft stays),
 *  null when it should go out as a message. */
export function useSteerCommand(input: {
  sessionId: string
  /** The session's agent CLI (the catalog `/effort` is looked up in). */
  agent: string | null | undefined
}): (text: string) => Promise<SteerDispatch> {
  const { sessionId, agent } = input
  return useCallback(
    async (text: string) => {
      const effort = effortLevel(text, agent)
      if (effort === null) return null
      try {
        await trpc.sessions.configure.mutate({ sessionId, effort }, SKIP_ERROR_TOAST)
        return `sent`
      } catch (error) {
        failedAction(`set`, `the effort`, error)
        return `failed`
      }
    },
    [sessionId, agent]
  )
}
