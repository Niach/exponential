import { useEffect, useRef, useState } from "react"
import { Button, Spinner, Textarea, conceptIcon, cn, toast } from "@exp/ui"
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

/** Review T9: the reply's note when the session it would resume cannot come
 *  back (×4: desktop `SESSION_ENDED_COPY`, Android `THREAD_SESSION_ENDED`,
 *  iOS `endedTitle` + `endedDescription`). */
export const THREAD_SESSION_ENDED_COPY = `That session ended. Your reply starts a new session in this thread.`

export const THREAD_REPLY_PLACEHOLDER = `Reply in this thread`

/** Review T10: how long a held reply waits for its resumed session (the
 *  composer start's deadline). */
export const THREAD_REPLY_RESUME_TIMEOUT_MS = 30_000
export const THREAD_REPLY_RESUME_TIMEOUT_COPY = `It did not come back in time. Your reply is back in the box.`

interface HeldReply {
  text: string
  fromRunId: string
  startId: string
}

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
  // Text waiting for the resumed session (sent once it is live), with the
  // start id its Retry reuses.
  const [held, setHeld] = useState<HeldReply | null>(null)
  const heldRef = useRef(held)
  heldRef.current = held

  // A held reply goes to the FIRST live session of mine that is not the one
  // it resumed — the resume, once its row and its stream are up.
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

  // Review T10: a resume that never comes up (the machine is offline, it
  // refused) times out like the composer's start: the text comes back and a
  // toast offers Retry (the same start id, so the machine never runs it
  // twice).
  useEffect(() => {
    if (!held) return
    const timer = setTimeout(() => {
      if (heldRef.current !== held) return
      setHeld(null)
      setText((current) => (current.trim() ? current : held.text))
      failedAction(`resume`, `the session`, undefined, {
        description: THREAD_REPLY_RESUME_TIMEOUT_COPY,
        retry: () => void send(held.text, held.startId),
      })
    }, THREAD_REPLY_RESUME_TIMEOUT_MS)
    return () => clearTimeout(timer)
    // `send` reads the latest target; the timer is per held reply.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [held])

  const send = async (message: string, reuseStartId?: string) => {
    if (!message || busy) return
    if (target.kind === `start` || (target.kind === `resume` && !resumeRun?.deviceId)) {
      // Review T9: nothing of mine to resume here (or its machine is
      // unknown): a new session in this thread. The ended one says so (the
      // same words ×4).
      if (target.kind === `resume`) toast.message(THREAD_SESSION_ENDED_COPY)
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
      setHeld({ text: message, fromRunId: resumeRun.id, startId })
      setText(``)
    } catch (error) {
      setText((current) => (current.trim() ? current : message))
      failedAction(`resume`, `the session`, error, {
        retry: () => void send(message, startId),
      })
    } finally {
      setBusy(false)
    }
  }
  const submit = () => send(text.trim())

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
