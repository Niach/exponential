// EXP-1216: the pure half of `exponential_sessions_messages` — a run's relay
// activity log (what `steer-transcript.ts` collected off a viewer socket)
// folded into the flat message list an MCP client can read.
//
// TOP-LEVEL TRANSCRIPT ONLY: every row carrying `subagentId` (a subagent's
// prose, turns and tool calls, which the apps render inside its card) is
// left out, and so is everything that is not a transcript row (latest-wins
// state such as `usage`/`config_state`/`turn`, diffs, subagent edges,
// compaction strips, acks). Narration fragments merge the way the feed
// merges them (`mergeNarrationFragment`, EXP-772): a fragment extends the
// previous top-level message when that is an assistant message with the same
// `messageId`; the merged message takes the LAST fragment's seq, so a client
// polling with `since` sees a still-growing message again, whole.

export type TranscriptRole = `user` | `assistant` | `tool` | `question`

export interface TranscriptMessage {
  /** The relay seq of the event (the last fragment's, for merged prose). */
  seq: number
  at?: number
  role: TranscriptRole
  text?: string
  /** Tool rows: the tool's name, its one-line detail and ACP kind. */
  name?: string
  detail?: string
  kind?: string
  /** Tool rows: the call settled as failed (its `tool_update`). */
  failed?: true
  /** Question rows: the option labels the card offers (≤10), and the
   *  ExitPlanMode plan-approval marker. */
  options?: string[]
  planMode?: true
  /** Assistant rows: the agent's message id the fragments merged by. */
  messageId?: string
}

/** One `activity` frame's payload: the publisher's seq + the event. */
export interface SeqEvent {
  seq: number
  event: Record<string, unknown>
}

function str(value: unknown): string | undefined {
  return typeof value === `string` ? value : undefined
}

function num(value: unknown): number | undefined {
  return typeof value === `number` && Number.isFinite(value) ? value : undefined
}

export function projectTranscript(
  events: readonly SeqEvent[]
): TranscriptMessage[] {
  const ordered = [...events].sort((a, b) => a.seq - b.seq)
  const messages: TranscriptMessage[] = []
  // tool call id → its message, for the failed settle.
  const tools = new Map<string, TranscriptMessage>()
  for (const { seq, event } of ordered) {
    if (str(event.subagentId)) continue
    const at = num(event.at)
    const stamp = at === undefined ? {} : { at }
    switch (event.kind) {
      case `narration`: {
        const text = str(event.text) ?? ``
        const messageId = str(event.messageId)
        const last = messages[messages.length - 1]
        if (
          messageId &&
          last?.role === `assistant` &&
          last.messageId === messageId
        ) {
          last.text = `${last.text ?? ``}${text}`
          last.seq = seq
          break
        }
        messages.push({
          seq,
          ...stamp,
          role: `assistant`,
          text,
          ...(messageId ? { messageId } : {}),
        })
        break
      }
      case `user_message`:
        messages.push({
          seq,
          ...stamp,
          role: `user`,
          text: str(event.text) ?? ``,
        })
        break
      case `tool`: {
        const name = str(event.name)
        const detail = str(event.detail)
        const kind = str(event.toolKind)
        const message: TranscriptMessage = {
          seq,
          ...stamp,
          role: `tool`,
          ...(name !== undefined ? { name } : {}),
          ...(detail ? { detail } : {}),
          ...(kind ? { kind } : {}),
        }
        messages.push(message)
        const id = str(event.id)
        if (id) tools.set(id, message)
        break
      }
      case `tool_update`: {
        // Only the failed settle survives the projection; diffs, outputs and
        // previews are the apps' business.
        const id = str(event.id)
        const target = id ? tools.get(id) : undefined
        if (target && event.status === `failed`) target.failed = true
        break
      }
      case `question`: {
        const options = Array.isArray(event.options)
          ? event.options
              .map((option) =>
                option && typeof option === `object`
                  ? str((option as Record<string, unknown>).label)
                  : undefined
              )
              .filter((label): label is string => label !== undefined)
              .slice(0, 10)
          : []
        messages.push({
          seq,
          ...stamp,
          role: `question`,
          text: str(event.text) ?? ``,
          ...(options.length > 0 ? { options } : {}),
          ...(event.planMode === true ? { planMode: true as const } : {}),
        })
        break
      }
      default:
        break
    }
  }
  return messages
}
