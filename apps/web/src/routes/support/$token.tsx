import { useCallback, useEffect, useRef, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { LifeBuoy, Loader2, Send } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { PoweredByFooter } from "@/components/team/powered-by-footer"
import { relativeTime } from "@/components/comment-rows/format"

// The reporter's magic-link conversation page (EXP-128). No login — the
// /support/<token> URL from the email IS the credential, so the page is
// mobile-first (opened from mail apps), noindex by the root default, and
// never leaks the URL onward: server-bun.ts answers /support/* with
// Referrer-Policy: no-referrer, and the meta tag below covers SPA-side
// navigations in dev.
//
// The page is LIVE (EXP-237): while the tab is visible it polls
// /api/support/poll every few seconds with a createdAt cursor, so member
// replies appear without a reload — and each poll heartbeats
// last_reporter_seen_at, which is what lets the server skip the "new reply"
// email while the reporter is watching. Hiding the tab pauses the poll, the
// heartbeat lapses, and emails resume.
export const Route = createFileRoute(`/support/$token`)({
  ssr: false,
  head: () => ({
    meta: [{ name: `referrer`, content: `no-referrer` }],
  }),
  component: SupportConversationPage,
})

interface ThreadMessage {
  id: string
  direction: `inbound` | `outbound`
  body: string
  createdAt: string
}

interface ThreadData {
  subject: string
  boardName: string | null
  teamName: string | null
  closed: boolean
  reporterName: string | null
  messages: ThreadMessage[]
}

type LoadState =
  | { kind: `loading` }
  | { kind: `notFound` }
  | { kind: `error` }
  | { kind: `ready`; thread: ThreadData }

const POLL_INTERVAL_MS = 5_000
const POLL_MAX_BACKOFF_MS = 60_000

function SupportConversationPage() {
  const { token } = Route.useParams()
  const [state, setState] = useState<LoadState>({ kind: `loading` })
  const [draft, setDraft] = useState(``)
  const [sending, setSending] = useState(false)
  const [sendError, setSendError] = useState<string | null>(null)
  const bottomRef = useRef<HTMLDivElement | null>(null)

  const load = useCallback(async () => {
    try {
      const res = await fetch(`/api/support/thread`, {
        method: `POST`,
        headers: { "content-type": `application/json` },
        body: JSON.stringify({ token }),
      })
      if (res.status === 404) {
        setState({ kind: `notFound` })
        return
      }
      if (!res.ok) {
        setState({ kind: `error` })
        return
      }
      const thread = (await res.json()) as ThreadData
      setState({ kind: `ready`, thread })
    } catch {
      setState({ kind: `error` })
    }
  }, [token])

  const stateRef = useRef(state)
  stateRef.current = state

  useEffect(() => {
    void load()
  }, [load])

  // Live updates (EXP-237): a self-rescheduling timeout chain (not
  // setInterval, so failures can back off) polling /api/support/poll with the
  // newest createdAt as cursor. Hiding the tab clears the timer entirely —
  // zero requests, and the server-side presence heartbeat lapses so reply
  // emails resume. Poll failures never touch the UI; the page keeps its
  // last-good transcript.
  useEffect(() => {
    if (state.kind !== `ready`) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | null = null
    let failures = 0

    const schedule = (delay: number) => {
      if (cancelled) return
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => void tick(), delay)
    }

    const tick = async () => {
      if (cancelled || document.visibilityState !== `visible`) return
      const current = stateRef.current
      if (current.kind !== `ready`) return
      const messages = current.thread.messages
      const since = messages[messages.length - 1]?.createdAt
      try {
        const res = await fetch(`/api/support/poll`, {
          method: `POST`,
          headers: { "content-type": `application/json` },
          body: JSON.stringify({ token, since }),
        })
        // Thread gone — stop polling for good.
        if (res.status === 404) return
        if (!res.ok) throw new Error(`poll failed`)
        const data = (await res.json()) as {
          closed: boolean
          messages: ThreadMessage[]
        }
        failures = 0
        if (!cancelled) {
          setState((prev) => {
            if (prev.kind !== `ready`) return prev
            // The gte cursor overlaps by design — dedupe by id.
            const seen = new Set(prev.thread.messages.map((m) => m.id))
            const fresh = data.messages.filter((m) => !seen.has(m.id))
            if (fresh.length === 0 && data.closed === prev.thread.closed) {
              return prev
            }
            return {
              kind: `ready`,
              thread: {
                ...prev.thread,
                closed: data.closed,
                messages: [...prev.thread.messages, ...fresh],
              },
            }
          })
        }
        schedule(POLL_INTERVAL_MS)
      } catch {
        failures += 1
        schedule(
          Math.min(POLL_INTERVAL_MS * 2 ** failures, POLL_MAX_BACKOFF_MS)
        )
      }
    }

    const onVisibility = () => {
      if (document.visibilityState === `visible`) {
        void tick()
      } else if (timer) {
        clearTimeout(timer)
        timer = null
      }
    }

    document.addEventListener(`visibilitychange`, onVisibility)
    schedule(POLL_INTERVAL_MS)
    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
      document.removeEventListener(`visibilitychange`, onVisibility)
    }
  }, [state.kind, token])

  // Keyed on the message count (not the whole state) so the 5s poll doesn't
  // yank the scroll position when nothing new arrived.
  const messageCount =
    state.kind === `ready` ? state.thread.messages.length : 0
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: `end` })
  }, [state.kind, messageCount])

  const send = async () => {
    const body = draft.trim()
    if (!body || sending) return
    setSending(true)
    setSendError(null)
    try {
      const res = await fetch(`/api/support/reply`, {
        method: `POST`,
        headers: { "content-type": `application/json` },
        body: JSON.stringify({ token, body }),
      })
      if (res.ok) {
        setDraft(``)
        await load()
      } else if (res.status === 409) {
        setSendError(`This conversation has been closed.`)
        await load()
      } else if (res.status === 404) {
        setSendError(`This conversation no longer exists.`)
      } else if (res.status === 429) {
        setSendError(`Too many messages — please wait a moment and try again.`)
      } else {
        setSendError(`Sending failed — please try again.`)
      }
    } catch {
      setSendError(`Sending failed — please check your connection.`)
    } finally {
      setSending(false)
    }
  }

  if (state.kind === `loading`) {
    return (
      <div className="flex min-h-svh items-center justify-center">
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (state.kind === `notFound` || state.kind === `error`) {
    return (
      <div className="flex min-h-svh flex-col">
        <div className="mx-auto flex w-full max-w-lg flex-1 flex-col items-center justify-center gap-3 px-4 text-center">
          <LifeBuoy className="h-8 w-8 text-muted-foreground" />
          <h1 className="text-lg font-semibold">
            {state.kind === `notFound`
              ? `Conversation not found`
              : `Something went wrong`}
          </h1>
          <p className="text-sm text-muted-foreground">
            {state.kind === `notFound`
              ? `This link doesn't match any conversation. Check that the URL from your email was copied completely.`
              : `Please try again in a moment.`}
          </p>
        </div>
        <PoweredByFooter />
      </div>
    )
  }

  const { thread } = state
  return (
    <div className="flex min-h-svh flex-col">
      <header className="border-b px-4 py-3">
        <div className="mx-auto flex w-full max-w-lg items-center gap-2">
          <LifeBuoy className="h-4 w-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0">
            <h1 className="truncate text-sm font-semibold">{thread.subject}</h1>
            <p className="truncate text-xs text-muted-foreground">
              {thread.boardName ?? thread.teamName ?? `Support`} support
            </p>
          </div>
        </div>
      </header>

      <main className="mx-auto flex w-full max-w-lg flex-1 flex-col gap-3 px-4 py-4">
        {thread.messages.map((message) => (
          <div
            key={message.id}
            className={
              message.direction === `inbound`
                ? `max-w-[85%] self-end rounded-2xl rounded-br-sm bg-primary px-3.5 py-2.5 text-sm text-primary-foreground`
                : `max-w-[85%] self-start rounded-2xl rounded-bl-sm bg-muted px-3.5 py-2.5 text-sm`
            }
          >
            <p className="whitespace-pre-wrap break-words">{message.body}</p>
            <p
              className={`mt-1 text-[0.65rem] ${
                message.direction === `inbound`
                  ? `text-primary-foreground/70`
                  : `text-muted-foreground`
              }`}
            >
              {message.direction === `inbound`
                ? (thread.reporterName ?? `You`)
                : `Support`}{` `}
              · {relativeTime(message.createdAt)}
            </p>
          </div>
        ))}
        <div ref={bottomRef} />
      </main>

      <div className="sticky bottom-0 border-t bg-background px-4 py-3">
        <div className="mx-auto w-full max-w-lg">
          {thread.closed ? (
            <p className="py-1 text-center text-sm text-muted-foreground">
              This conversation is closed. Need anything else? Open a new
              request from where you first reached out.
            </p>
          ) : (
            <div className="flex items-end gap-2">
              <Textarea
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === `Enter` && !event.shiftKey) {
                    event.preventDefault()
                    void send()
                  }
                }}
                placeholder="Write a reply…"
                rows={2}
                className="min-h-9 flex-1 resize-none"
              />
              <Button
                size="icon"
                className="h-9 w-9 shrink-0"
                disabled={sending || draft.trim().length === 0}
                onClick={() => void send()}
                aria-label="Send reply"
              >
                {sending ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Send className="h-4 w-4" />
                )}
              </Button>
            </div>
          )}
          {sendError && (
            <p className="mt-2 text-xs text-destructive">{sendError}</p>
          )}
        </div>
      </div>

      <PoweredByFooter />
    </div>
  )
}
