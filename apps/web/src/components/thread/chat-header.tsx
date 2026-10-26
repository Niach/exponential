import type { ReactNode } from "react"
import { CHATS_COPY, type ChatLead } from "@exp/domain-contract/chats"
import { ChatLeadDisc, PhoneHeaderTitle } from "@exp/ui"
import { cn } from "@/lib/utils"

// Thread 58cb32df (chats.json, the draft's chat screen; iOS/Android's chat
// header): the phone header's lead = a 40px DISC (the chat's lead: the run
// mark with the amber badge / the working mark / the PR glyph; an agent's
// icon in its colour) beside the title over ONE subtitle line (a chat: its
// state word, amber while it needs you, then ` · ` the device; an agent:
// `Agent`). The back button, Stop and `…` stay the header's own.

/** The disc's size on the header (the list's is `shell.leadDisc`). */
const HEADER_DISC_CLASS = `!size-10 [&>svg]:size-5! [&>[data-slot=run-mark-badge]]:right-1.5! [&>[data-slot=run-mark-badge]]:top-1.5!`

export { AgentHeaderTitle, CHAT_HEADER_AGENT_SUBTITLE } from "@exp/ui"

/** A chat's phone header title: the lead disc, the title, the state word
 *  (`needs input` amber / `working…` foreground) · the device. */
export function ChatHeaderTitle({
  lead,
  agent,
  title,
  trailing,
  device,
  needsYou,
  busy,
  needsYouWord,
}: {
  lead: ChatLead
  agent: string | null
  title: ReactNode
  /** After the title (the private lock). */
  trailing?: ReactNode
  device: string | null
  needsYou: boolean
  busy: boolean
  /** The `needs input` word (threads-list.json `words.needsInput`). */
  needsYouWord: string
}) {
  const word = needsYou ? needsYouWord : busy ? CHATS_COPY.preview.working : null
  const tone = needsYou ? `text-amber-400` : busy ? `text-foreground` : `text-muted-foreground`
  const parts = [word, device].filter(Boolean).join(` · `)
  return (
    <PhoneHeaderTitle
      disc={<ChatLeadDisc lead={lead} agent={agent} className={HEADER_DISC_CLASS} />}
      title={
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="truncate">{title}</span>
          {trailing}
        </span>
      }
      subtitle={parts ? <span className={cn(word ? tone : `text-muted-foreground`)}>{parts}</span> : null}
    />
  )
}
