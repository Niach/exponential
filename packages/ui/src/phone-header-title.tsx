import type { ReactNode } from "react"
import { BoardGlyph } from "./board-glyph"

// NEW-15: the phone header's title block (moved from apps/web
// components/thread/chat-header.tsx, which re-exports it), so the marketing
// site's phone demos draw the agent header the app draws. Thread 58cb32df
// (chats.json): a 40px DISC beside the title over ONE subtitle line.

export const CHAT_HEADER_AGENT_SUBTITLE = `Agent`

export function PhoneHeaderTitle({
  disc,
  title,
  subtitle,
}: {
  disc: ReactNode
  title: ReactNode
  subtitle: ReactNode
}) {
  return (
    <span className="flex min-w-0 items-center gap-2.5 text-left" data-testid="chat-header-title">
      {disc}
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[15px] font-semibold text-foreground">{title}</span>
        {subtitle ? (
          <span className="truncate text-xs font-normal" data-testid="chat-header-subtitle">
            {subtitle}
          </span>
        ) : null}
      </span>
    </span>
  )
}

/** An agent chat's phone header title: its icon in its colour on the disc,
 *  the name over `Agent`. */
export function AgentHeaderTitle({
  name,
  icon,
  color,
}: {
  name: string
  icon: string | null
  color: string | null
}) {
  return (
    <PhoneHeaderTitle
      disc={
        <span
          data-slot="chat-lead"
          className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted"
        >
          <BoardGlyph board={{ icon, color }} className="size-5" />
        </span>
      }
      title={name}
      subtitle={<span className="text-muted-foreground">{CHAT_HEADER_AGENT_SUBTITLE}</span>}
    />
  )
}
