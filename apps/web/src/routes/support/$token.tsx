import { useCallback, useEffect, useRef, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { LifeBuoy, Loader2, Send } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { PoweredByFooter } from "@/components/workspace/powered-by-footer"
import { relativeTime } from "@/components/comment-rows/format"

// The reporter's magic-link conversation page (EXP-128). No login — the
// /support/<token> URL from the email IS the credential, so the page is
// mobile-first (opened from mail apps), noindex by the root default, and
// never leaks the URL onward: server-bun.ts answers /support/* with
// Referrer-Policy: no-referrer, and the meta tag below covers SPA-side
// navigations in dev.
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
  projectName: string | null
  workspaceName: string | null
  closed: boolean
  reporterName: string | null
  messages: ThreadMessage[]
}

type LoadState =
  | { kind: `loading` }
  | { kind: `notFound` }
  | { kind: `error` }
  | { kind: `ready`; thread: ThreadData }

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

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: `end` })
  }, [state])

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
              {thread.projectName ?? thread.workspaceName ?? `Support`} support
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
