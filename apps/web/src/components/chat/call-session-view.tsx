import { useMemo } from "react"
import { CHAT_COLUMN_CLASS, CallViewBody, WORK_GUTTER_CLASS, userMessageTime } from "@exp/ui"
import { callRunning, callView, type CallView } from "@exp/domain-contract/session-tree"
import { renderChatText } from "@/components/chat/chat-markdown"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import { useSessionMessages } from "@/hooks/use-session-tree"
import { toTreeMessages, toTreeSession, type SessionRow } from "@/lib/session-tree"
import { cn } from "@/lib/utils"

const attachmentUrl = (attachmentId: string) => `/api/attachments/${attachmentId}`

// #EXP-1943 (session-card.json `call`, `callView`): an opened call session
// = THE call view (`@exp/ui` CallViewBody) over its own messages.

/** The call view of `row` and its rows' times (null = not a call session). */
export function useCallView(
  row: SessionRow | null,
  agentName: string
): { view: CallView; times: ReadonlyMap<string, string> } | null {
  const call = row?.voiceCall ? row : null
  const messages = useSessionMessages(call?.id ?? null)
  const tree = useMemo(() => (call ? toTreeSession(call) : null), [call])
  const now = useNow(tree && callRunning(tree) ? 1_000 : 60_000)
  return useMemo(() => {
    if (!tree) return null
    const rows = toTreeMessages(messages)
    const view = callView({
      session: tree,
      messages: rows,
      agentName,
      now: now.toISOString(),
      utcOffsetMinutes: -now.getTimezoneOffset(),
    })
    return { view, times: new Map(rows.map((message) => [message.id, message.createdAt])) }
  }, [tree, messages, agentName, now])
}

export function CallSessionBody({
  view,
  agentId,
  agentName,
  heading,
  messageTimes,
}: {
  view: CallView
  agentId: string | null
  agentName: string
  /** Wide: the title + subtitle on top of the column (phones: the header). */
  heading: boolean
  messageTimes: ReadonlyMap<string, string>
}) {
  const { data: auth } = useSession()
  const viewer = auth?.user ?? null
  return (
    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain" data-testid="call-session">
      <CallViewBody
        className={cn(CHAT_COLUMN_CLASS, WORK_GUTTER_CLASS, `py-4`)}
        view={view}
        heading={heading}
        renderText={renderChatText}
        attachmentSrc={attachmentUrl}
        chat={{ sessionId: view.sessionId, title: agentName, agentId }}
        viewerId={viewer?.id ?? null}
        viewer={
          viewer ? { id: viewer.id, name: viewer.name ?? null, email: viewer.email ?? null, image: viewer.image ?? null } : null
        }
        messageTime={(row) => {
          const at = messageTimes.get(row.id)
          return at ? userMessageTime(new Date(at)) : null
        }}
      />
    </div>
  )
}
