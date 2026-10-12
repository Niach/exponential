import { useEffect, useRef, useState } from "react"
import { Button, Spinner, Textarea, conceptIcon, cn } from "@exp/ui"
import type { CodingSession } from "@/db/schema"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { acquireSteerSession } from "@/lib/steer-session-store"
import type { ThreadReplyTarget } from "@/lib/thread-page"
import { trpc } from "@/lib/trpc-client"

// EXP-1284: `Reply in this thread` — the thread page's composer. Where the
// text goes is `threadReplyTarget` (lib/thread-page.ts):
//   send    the latest LIVE run the viewer owns: the existing steer send
//           (the run's store, shared with its body above);
//   resume  their run ended: `steer.startSession({resumeSessionId})` (the
//           resume stays in the thread), and the text is HELD until that
//           resumed run is live in the thread, then sent to it;
//   start   otherwise a new run in this thread: the New run tab (the
//           composer, with its device and agent pickers) prefilled with the
//           thread and the text (`onStart`).

const SendIcon = conceptIcon(`ui-send`)

export const THREAD_REPLY_PLACEHOLDER = `Reply in this thread`

export function ThreadReplyComposer({
  target,
  resumeRun,
  onStart,
  className,
}: {
  target: ThreadReplyTarget
  /** The run a `resume` target relaunches (its device). */
  resumeRun: CodingSession | null
  /** `start`: open the composer seeded with this text. */
  onStart: (text: string) => void
  className?: string
}) {
  const [text, setText] = useState(``)
  const [busy, setBusy] = useState(false)
  // Text waiting for the resumed run (sent once it is live).
  const [held, setHeld] = useState<{ text: string; fromRunId: string } | null>(null)
  const heldRef = useRef(held)
  heldRef.current = held

  // A held reply goes to the FIRST live run of mine that is not the one it
  // resumed — the resume, once its row and its stream are up.
  const sendRunId = target.kind === `send` ? target.runId : null
  useEffect(() => {
    if (!held || !sendRunId || sendRunId === held.fromRunId) return
    const store = acquireSteerSession(sendRunId)
    store.connect()
    const trySend = () => {
      const pending = heldRef.current
      if (!pending) return
      if (store.getSnapshot().phase.kind !== `live`) return
      if (store.sendMessage(pending.text)) setHeld(null)
    }
    trySend()
    return store.subscribe(trySend)
  }, [held, sendRunId])

  const submit = async (reuseStartId?: string) => {
    const message = text.trim()
    if (!message || busy) return
    if (target.kind === `start`) {
      onStart(message)
      setText(``)
      return
    }
    if (target.kind === `send`) {
      const sent = acquireSteerSession(target.runId).sendMessage(message)
      if (!sent) {
        failedAction(`send`, `the message`, undefined, {
          description: `The session is not connected yet. Try again in a moment.`,
        })
        return
      }
      setText(``)
      return
    }
    if (!resumeRun?.deviceId) return
    setBusy(true)
    // The batch's idempotent start id: this press mints one, its Retry
    // reuses it (the device ignores a start it already ran). A resume never
    // names a thread: it stays in its predecessor's.
    const startId = reuseStartId ?? crypto.randomUUID()
    try {
      await trpc.steer.startSession.mutate(
        { resumeSessionId: resumeRun.id, deviceId: resumeRun.deviceId, startId },
        SKIP_ERROR_TOAST
      )
      setHeld({ text: message, fromRunId: resumeRun.id })
      setText(``)
    } catch (error) {
      failedAction(`resume`, `the session`, error, { retry: () => void submit(startId) })
    } finally {
      setBusy(false)
    }
  }

  const pending = busy || held !== null
  return (
    <div
      data-testid="thread-reply"
      data-target={target.kind}
      className={cn(
        `relative flex items-end gap-2 rounded-2xl border border-border bg-card/80 px-4 py-2.5 shadow-sm`,
        className
      )}
    >
      <Textarea
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === `Enter` && !event.shiftKey && !event.nativeEvent.isComposing) {
            event.preventDefault()
            void submit()
          }
        }}
        rows={1}
        placeholder={THREAD_REPLY_PLACEHOLDER}
        aria-label={THREAD_REPLY_PLACEHOLDER}
        className="max-h-40 min-h-7 flex-1 resize-none border-0 bg-transparent! p-0 py-1 text-sm shadow-none focus-visible:ring-0"
      />
      {(text.trim() || pending) && (
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          aria-label="Send"
          disabled={pending || !text.trim()}
          onClick={() => void submit()}
          className="shrink-0"
        >
          {pending ? <Spinner size={16} /> : <SendIcon className="size-4" />}
        </Button>
      )}
    </div>
  )
}
