import type { ReactNode } from "react"
import { cn } from "./cn"
import type { SessionThread } from "./session-results"
import { SessionInlineResultTile } from "./session-results-view"

// EXP-1175: the Run face's default body — the run's published results in
// publish order (`sessionThread`): a text item = its topic caption over the
// app-rendered GFM, a picture = the inline result tile; the Summary text
// LAST as the agent's reply; then whatever the host passes as children (the
// pending plan/question cards).

export function SessionThreadView({
  thread,
  attachmentSrc,
  renderText,
  renderReply,
  children,
  className,
}: {
  thread: SessionThread
  attachmentSrc: (attachmentId: string) => string
  renderText: (text: string) => ReactNode
  renderReply: (text: string) => ReactNode
  children?: ReactNode
  className?: string
}) {
  return (
    <div
      data-testid="session-thread"
      className={cn(`space-y-3 pt-2`, className)}
    >
      {thread.items.map((item, index) =>
        item.kind === `text` ? (
          <div key={`text-${item.topic}-${index}`} data-slot="session-thread-text">
            <div className="mb-1 text-[0.6875rem] text-muted-foreground">
              {item.topic}
            </div>
            {renderText(item.text)}
          </div>
        ) : (
          <SessionInlineResultTile
            key={`picture-${item.entry.attachmentId}-${index}`}
            entry={item.entry}
            attachmentSrc={attachmentSrc}
          />
        )
      )}
      {thread.reply !== null && (
        <div data-slot="session-thread-reply">{renderReply(thread.reply)}</div>
      )}
      {children}
    </div>
  )
}
