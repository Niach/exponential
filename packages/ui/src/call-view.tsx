import type { ComponentProps, ReactNode } from "react"
import type { CallView } from "@exp/domain-contract/session-tree"
import { GlassSectionHeader } from "./glass-rows"
import { ChatConversation } from "./session-chat-view"
import { cn } from "./cn"

/**
 * #EXP-1943 (session-card.json `call`, `callView`) THE call view ×4: a call
 * session's own screen. The heading (title over the muted subtitle) where
 * the screen's header does not carry it, the summary under the section band
 * with a hairline under it, then the spoken turns as the chat's bubbles.
 * Existing parts only: GlassSectionHeader + ChatConversation.
 */
export function CallViewBody({
  view,
  heading,
  renderText,
  attachmentSrc,
  chat,
  viewerId,
  viewer,
  member,
  messageTime,
  arrived,
  className,
}: {
  view: CallView
  /** Draw the title + subtitle on top (wide; phones: the header has them). */
  heading: boolean
  renderText: (text: string) => ReactNode
  attachmentSrc: (attachmentId: string) => string
  /** The call's chat agent (the bubbles' sender). */
  chat: { sessionId: string; title: string; agentId: string | null }
} & Pick<
  ComponentProps<typeof ChatConversation>,
  `viewerId` | `viewer` | `member` | `messageTime` | `arrived` | `className`
>) {
  return (
    <div data-testid="call-view" className={cn(`flex flex-col gap-4`, className)}>
      {heading && (
        <div className="flex min-w-0 flex-col" data-testid="call-view-heading">
          <span className="truncate text-sm font-semibold text-foreground">{view.title}</span>
          <span className="truncate text-xs text-muted-foreground">{view.subtitle}</span>
        </div>
      )}
      {view.summary && (
        <div className="flex flex-col border-b border-border pb-4" data-testid="call-view-summary">
          <GlassSectionHeader label={view.summaryLabel} className="-mx-3 w-auto md:mx-0" />
          <div className="pt-2">{renderText(view.summary)}</div>
        </div>
      )}
      <ChatConversation
        rows={view.rows}
        layout="bubbles"
        renderText={renderText}
        attachmentSrc={attachmentSrc}
        chat={chat}
        viewerId={viewerId}
        viewer={viewer}
        member={member}
        messageTime={messageTime}
        arrived={arrived}
      />
    </div>
  )
}
