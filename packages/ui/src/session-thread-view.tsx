import type { ReactNode } from "react"
import { cn } from "./cn"
import type {
  SessionThread,
  SessionThreadItem,
  SessionTurn,
  SessionTurnMessage,
} from "./session-results"
import { SessionInlineResultTile } from "./session-results-view"
import { UserMessageBubble } from "./user-message-bubble"

// EXP-1175: the Run face's default body — the run's published results in
// publish order (`sessionThread`): a text item = its topic caption over the
// app-rendered GFM, a picture = the inline result tile; the Summary text
// LAST as the agent's reply; then whatever the host passes as children (the
// pending plan/question cards).
//
// EXP-1245: for the run's OWNER the thread is a conversation of TURNS
// (`sessionTurns`): the person's message as a right-aligned bubble, the
// turn's own status row (the host renders `RunStatusRow` off
// `turnRowCaption`), then that turn's items and reply, indented under the
// row's caption. Tool noise stays behind Show work.

function ThreadItems({
  items,
  reply,
  attachmentSrc,
  renderText,
  renderReply,
}: {
  items: readonly SessionThreadItem[]
  reply: string | null
  attachmentSrc: (attachmentId: string) => string
  renderText: (text: string) => ReactNode
  renderReply: (text: string) => ReactNode
}) {
  return (
    <>
      {items.map((item, index) =>
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
      {reply !== null && (
        <div data-slot="session-thread-reply">{renderReply(reply)}</div>
      )}
    </>
  )
}

export function SessionThreadView({
  thread,
  turns,
  renderStatusRow,
  messageCaption,
  attachmentSrc,
  renderText,
  renderReply,
  children,
  className,
}: {
  /** Today's single thread (no feed: teammates, an offline host). */
  thread?: SessionThread
  /** EXP-1245: the owner's turns; wins over `thread`. */
  turns?: readonly SessionTurn[]
  /** EXP-1245: a turn's status row (null = none, e.g. a waiting message). */
  renderStatusRow?: (turn: SessionTurn, index: number) => ReactNode
  /** EXP-1245: the caption under the person's bubble. */
  messageCaption?: (message: SessionTurnMessage) => string | null
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
      {turns
        ? turns.map((turn, index) => {
            const row = renderStatusRow?.(turn, index) ?? null
            const hasBody = turn.items.length > 0 || turn.reply !== null
            return (
              <div key={`turn-${index}`} data-slot="session-turn" className="space-y-3">
                {turn.message && (
                  <UserMessageBubble
                    text={turn.message.text}
                    imageSrc={turn.message.images[0] ?? null}
                    files={turn.message.files}
                    caption={messageCaption?.(turn.message) ?? null}
                  />
                )}
                {row}
                {hasBody && (
                  <div className={cn(`space-y-3`, row !== null && `pl-6`)}>
                    <ThreadItems
                      items={turn.items}
                      reply={turn.reply}
                      attachmentSrc={attachmentSrc}
                      renderText={renderText}
                      renderReply={renderReply}
                    />
                  </div>
                )}
              </div>
            )
          })
        : thread && (
            <ThreadItems
              items={thread.items}
              reply={thread.reply}
              attachmentSrc={attachmentSrc}
              renderText={renderText}
              renderReply={renderReply}
            />
          )}
      {children}
    </div>
  )
}
