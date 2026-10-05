import { useState } from "react"
import { Button, Textarea, conceptIcon } from "@exp/ui"
import { useMcpActions } from "./actions"
import { steerFeedback } from "./run-model"

const SendIcon = conceptIcon(`ui-send`)

// EXP-1183 — the live run's composer: one line of text into the run through
// `exponential_sessions_message` (the steer view's input). Enter sends,
// Shift+Enter breaks the line; the answer says whether it landed or queued
// behind the agent's current step.
export function RunSteerComposer({ runId }: { runId: string }) {
  const { call } = useMcpActions()
  const [text, setText] = useState(``)
  const [sending, setSending] = useState(false)
  const [feedback, setFeedback] = useState<{ tone: `ok` | `error`; text: string } | null>(
    null
  )

  const send = async () => {
    const message = text.trim()
    if (!message || sending) return
    setSending(true)
    setFeedback(null)
    const result = await call(`exponential_sessions_message`, { id: runId, message })
    setSending(false)
    if (result.kind === `error`) {
      setFeedback({ tone: `error`, text: result.message })
      return
    }
    setText(``)
    setFeedback({ tone: `ok`, text: steerFeedback(result.data) })
  }

  return (
    <div
      className="sticky bottom-0 flex flex-col gap-1 border-t border-glass-stroke bg-background/85 px-7 py-2.5 backdrop-blur"
      data-testid="run-steer-composer"
    >
      <div className="flex items-end gap-2">
        <Textarea
          value={text}
          rows={1}
          placeholder="Message the run…"
          aria-label="Message the run"
          className="max-h-40 min-h-9 py-1.5 text-sm"
          disabled={sending}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === `Enter` && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault()
              void send()
            }
          }}
        />
        <Button
          variant="glass"
          size="icon-sm"
          aria-label="Send"
          title="Send"
          disabled={sending || text.trim().length === 0}
          onClick={() => void send()}
        >
          <SendIcon />
        </Button>
      </div>
      {feedback && (
        <p
          className={
            feedback.tone === `error`
              ? `text-xs text-destructive`
              : `text-xs text-muted-foreground`
          }
          role="status"
        >
          {feedback.text}
        </p>
      )}
    </div>
  )
}
