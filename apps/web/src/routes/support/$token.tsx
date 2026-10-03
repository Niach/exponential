import { useCallback, useEffect, useRef, useState } from "react"
import type { ReactNode } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { LifeBuoy, LoaderCircle } from "lucide-react"
import {
  Button,
  Pill,
  Textarea,
  conceptIcon,
  Composer,
  ComposerSubmit,
} from "@exp/ui"
import { PoweredByFooter } from "@/components/team/powered-by-footer"
import { relativeTime } from "@/components/comment-rows/format"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { unescapeReporterText } from "@/lib/reporter/report-text"

// The reporter's magic-link conversation page (EXP-128, SLOP-4: the
// conversation IS the issue's reporter-audience comments). No login — the
// /support/<token> URL from the email IS the credential, so the page is
// mobile-first (opened from mail apps), noindex by the root default, and
// never leaks the URL onward: server-bun.ts answers /support/* with
// Referrer-Policy: no-referrer, and the meta tag below covers SPA-side
// navigations in dev.
//
// The page loads once and reloads after a send (SLOP-4 retired the live
// poll); every member reply reaches the reporter by email with this link.
// What it shows is PLAIN TEXT: the server stored the reporter's words as
// backslash-escaped GFM for the member clients, and this page unescapes them
// back — member replies are shown as typed.
// EXP-317: the send glyph resolves through the shared registry.
const SendIcon = conceptIcon(`ui-send`)

export const Route = createFileRoute(`/support/$token`)({
  ssr: false,
  head: () => ({
    meta: [
      { title: pageTitle(`Support`) },
      { name: `referrer`, content: `no-referrer` },
    ],
  }),
  component: SupportConversationPage,
})

interface ThreadMessage {
  id: string
  direction: `inbound` | `outbound`
  body: string
  createdAt: string
}

interface ReportAttachment {
  id: string
  filename: string
  contentType: string
  width: number | null
  height: number | null
}

interface ThreadData {
  subject: string
  teamName: string | null
  status: `open` | `resolved`
  reporterName: string | null
  report: { title: string; description: string; createdAt: string }
  attachments: ReportAttachment[]
  messages: ThreadMessage[]
}

type LoadState =
  | { kind: `loading` }
  | { kind: `notFound` }
  | { kind: `error` }
  | { kind: `throttled`; retryIn: number }
  | { kind: `ready`; thread: ThreadData }

const RETRY_AFTER_FALLBACK_S = 5
const RETRY_AFTER_MAX_S = 60

// The read bucket is keyed per IP, so a whole carrier-grade NAT (i.e. the
// mail-app-on-phone audience) shares it. Honor the server's Retry-After, but
// never trust it far enough to strand the reporter on a countdown.
export function retryAfterSeconds(header: string | null): number {
  const parsed = Number.parseInt(header ?? ``, 10)
  if (!Number.isFinite(parsed) || parsed <= 0) return RETRY_AFTER_FALLBACK_S
  return Math.min(parsed, RETRY_AFTER_MAX_S)
}

function SupportConversationPage() {
  const { token } = Route.useParams()
  return <SupportConversationView token={token} />
}

export function SupportConversationView({ token }: { token: string }) {
  const [state, setState] = useState<LoadState>({ kind: `loading` })
  usePageTitle(
    state.kind === `ready` ? pageTitle(state.thread.subject) : undefined
  )
  const [draft, setDraft] = useState(``)
  const [sending, setSending] = useState(false)
  const [sendError, setSendError] = useState<string | null>(null)
  const bottomRef = useRef<HTMLDivElement | null>(null)

  // `refresh` = a transcript is already on screen (the post-send reload): a
  // failed refresh must never replace it with an error page.
  const load = useCallback(
    async ({ refresh = false }: { refresh?: boolean } = {}) => {
      try {
        const res = await fetch(`/api/support/thread`, {
          method: `POST`,
          headers: { "content-type": `application/json` },
          body: JSON.stringify({ token }),
        })
        if (res.ok) {
          const thread = (await res.json()) as ThreadData
          setState({ kind: `ready`, thread })
          return
        }
        if (refresh) return
        if (res.status === 404) {
          setState({ kind: `notFound` })
          return
        }
        if (res.status === 429) {
          setState({
            kind: `throttled`,
            retryIn: retryAfterSeconds(res.headers.get(`retry-after`)),
          })
          return
        }
        setState({ kind: `error` })
      } catch {
        if (!refresh) setState({ kind: `error` })
      }
    },
    [token]
  )

  const retry = useCallback(() => {
    setState({ kind: `loading` })
    void load()
  }, [load])

  const stateRef = useRef(state)
  stateRef.current = state

  useEffect(() => {
    void load()
  }, [load])

  // A throttled load is a shared-IP hiccup, not a broken page: count the
  // server's Retry-After down on screen and reload when it lapses (the
  // "Try again" button is the impatient path).
  useEffect(() => {
    if (state.kind !== `throttled`) return
    if (state.retryIn <= 0) {
      void load()
      return
    }
    const timer = setTimeout(() => {
      setState((prev) =>
        prev.kind === `throttled`
          ? { ...prev, retryIn: prev.retryIn - 1 }
          : prev
      )
    }, 1_000)
    return () => clearTimeout(timer)
  }, [state, load])

  // Keyed on the message count (not the whole state) so a refresh that
  // brought nothing new doesn't yank the scroll position.
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
        await load({ refresh: true })
      } else if (res.status === 404) {
        setSendError(`This conversation moved.`)
      } else if (res.status === 429) {
        setSendError(`Too many messages. Please wait a moment and try again.`)
      } else {
        setSendError(`Couldn't send your message. Please try again.`)
      }
    } catch {
      setSendError(`Couldn't send your message. Please check your connection.`)
    } finally {
      setSending(false)
    }
  }

  if (state.kind === `loading`) {
    return (
      <div className="flex min-h-svh items-center justify-center">
        <LoaderCircle className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (state.kind === `notFound`) {
    return (
      <StatusScreen
        title="This conversation moved"
        body="This conversation moved. Please write to us again from where you first reached out."
      />
    )
  }

  if (state.kind === `throttled`) {
    return (
      <StatusScreen
        title="We're busy right now"
        body={
          state.retryIn > 0
            ? `Too many requests came from your network. Retrying in ${state.retryIn}s…`
            : `Too many requests came from your network. Retrying…`
        }
        action={
          <Button variant="outline" size="sm" onClick={retry}>
            Try again
          </Button>
        }
      />
    )
  }

  if (state.kind === `error`) {
    return (
      <StatusScreen
        title="Something went wrong"
        body="We couldn't load this conversation. Please try again in a moment."
        action={
          <Button variant="outline" size="sm" onClick={retry}>
            Try again
          </Button>
        }
      />
    )
  }

  const { thread } = state
  return (
    <div className="flex min-h-svh flex-col">
      <header className="border-b px-4 py-3">
        <div className="mx-auto flex w-full max-w-lg items-center gap-2">
          <LifeBuoy className="h-4 w-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-sm font-semibold">
              {unescapeReporterText(thread.subject)}
            </h1>
            <p className="truncate text-xs text-muted-foreground">
              Your report to {thread.teamName ?? `the team`}
            </p>
          </div>
          <Pill
            dot={thread.status === `resolved` ? `#22c55e` : `#a1a1aa`}
            data-testid="report-status"
          >
            {thread.status === `resolved` ? `Resolved` : `Open`}
          </Pill>
        </div>
      </header>

      <main className="mx-auto flex w-full max-w-lg flex-1 flex-col gap-3 px-4 py-4">
        <ReportBlock
          token={token}
          report={thread.report}
          attachments={thread.attachments}
        />
        {thread.messages.map((message) => (
          <div
            key={message.id}
            className={
              message.direction === `inbound`
                ? `max-w-[85%] self-end rounded-2xl rounded-br-sm bg-primary px-3.5 py-2.5 text-sm text-primary-foreground`
                : `max-w-[85%] self-start rounded-2xl rounded-bl-sm bg-muted px-3.5 py-2.5 text-sm`
            }
          >
            <p className="whitespace-pre-wrap break-words">
              {message.direction === `inbound`
                ? unescapeReporterText(message.body)
                : message.body}
            </p>
            {/* EXP-698: the reporter's own (outgoing) bubble is filled with
                `primary`, which is near-WHITE — the meta line has to be dark
                on it, not another light tint. */}
            <p
              className={`mt-1 text-[0.65rem] ${
                message.direction === `inbound`
                  ? `text-black/60`
                  : `text-muted-foreground`
              }`}
            >
              {message.direction === `inbound`
                ? (thread.reporterName ?? `You`)
                : (thread.teamName ?? `Support`)}{` `}
              · {relativeTime(message.createdAt)}
            </p>
          </div>
        ))}
        <div ref={bottomRef} />
      </main>

      {/* EXP-442: `bg-background` is the hue-less neutral, so this painted a
          rgb(10,10,10) bar across the bottom of a page whose gradient has
          already ramped to rgb(24,24,27) there — and whose own header is
          transparent. Stays OPAQUE rather than taking glass-chrome-bottom:
          the near-white inbound bubbles smear straight through a 70% scrim,
          and the Textarea is itself translucent, so the smear lands inside
          the input. Opaque at the bottom endpoint is the seamless one. */}
      <div className="sticky bottom-0 border-t bg-[var(--glass-background-bottom)] px-4 py-3">
        <div className="mx-auto w-full max-w-lg">
          {thread.status === `resolved` && (
            <p className="pb-2 text-center text-xs text-muted-foreground">
              This report was resolved. Replying reopens it.
            </p>
          )}
          {
            // EXP-698: the ONE composer card, shared with comments and
            // steering. `opaque` because it floats on the page's own
            // bottom-endpoint bar. Always shown: a reply reopens a resolved
            // report (SLOP-4).
            <Composer
              opaque
              submit={
                <ComposerSubmit
                  disabled={sending || draft.trim().length === 0}
                  onClick={() => void send()}
                  aria-label="Send reply"
                >
                  {sending ? (
                    <LoaderCircle className="size-5 animate-spin" />
                  ) : (
                    <SendIcon className="!size-6" />
                  )}
                </ComposerSubmit>
              }
            >
              <Textarea
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={(event) => {
                  // Enter always inserts a newline here: this page is opened
                  // from mail apps on phones, whose return key has no Shift —
                  // Enter-to-send would make multi-line replies impossible and
                  // burn the strict per-thread reply budget one fragment at a
                  // time. Sending is the button, or ⌘/Ctrl+Enter on a hardware
                  // keyboard (never mid-IME-composition).
                  if (
                    event.key === `Enter` &&
                    (event.metaKey || event.ctrlKey) &&
                    !event.nativeEvent.isComposing
                  ) {
                    event.preventDefault()
                    void send()
                  }
                }}
                placeholder="Write a reply…"
                rows={2}
                className="min-h-16 border-none bg-transparent text-sm shadow-none focus-visible:border-transparent dark:bg-transparent"
              />
            </Composer>
          }
          {sendError && (
            <p className="mt-2 text-xs text-destructive">{sendError}</p>
          )}
        </div>
      </div>

      <PoweredByFooter />
    </div>
  )
}

// The report itself: title, the reporter's text (image embeds stripped
// server-side, escapes dropped here) and their pictures. Pictures load
// through POST /api/support/attachment (token in the body, never in a URL)
// into object URLs, revoked on unmount.
function ReportBlock({
  token,
  report,
  attachments,
}: {
  token: string
  report: ThreadData[`report`]
  attachments: ReportAttachment[]
}) {
  const description = unescapeReporterText(report.description)
  return (
    <section className="rounded-2xl border bg-muted/40 px-3.5 py-3 text-sm">
      <p className="text-[0.65rem] text-muted-foreground">
        Your report · {relativeTime(report.createdAt)}
      </p>
      <h2 className="mt-1 font-semibold">{unescapeReporterText(report.title)}</h2>
      {description && (
        <p className="mt-1 whitespace-pre-wrap break-words">{description}</p>
      )}
      {attachments.length > 0 && (
        <div className="mt-3 flex gap-2 overflow-x-auto" data-testid="pictures">
          {attachments.map((attachment) => (
            <ReportPicture
              key={attachment.id}
              token={token}
              attachment={attachment}
            />
          ))}
        </div>
      )}
    </section>
  )
}

function ReportPicture({
  token,
  attachment,
}: {
  token: string
  attachment: ReportAttachment
}) {
  const [url, setUrl] = useState<string | null>(null)
  useEffect(() => {
    let objectUrl: string | null = null
    let cancelled = false
    void (async () => {
      try {
        const res = await fetch(`/api/support/attachment`, {
          method: `POST`,
          headers: { "content-type": `application/json` },
          body: JSON.stringify({ token, id: attachment.id }),
        })
        if (!res.ok || cancelled) return
        objectUrl = URL.createObjectURL(await res.blob())
        if (cancelled) {
          URL.revokeObjectURL(objectUrl)
          return
        }
        setUrl(objectUrl)
      } catch {
        // A missing picture leaves its slot empty; the report text stands.
      }
    })()
    return () => {
      cancelled = true
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [token, attachment.id])
  const aspect =
    attachment.width && attachment.height
      ? `${attachment.width} / ${attachment.height}`
      : undefined
  return (
    <a
      href={url ?? undefined}
      target="_blank"
      rel="noreferrer"
      className="block h-28 shrink-0 overflow-hidden rounded-lg border bg-muted"
      style={{ aspectRatio: aspect }}
    >
      {url ? (
        <img
          src={url}
          alt={attachment.filename}
          className="h-full w-full object-cover"
        />
      ) : (
        <div className="flex h-full w-28 items-center justify-center">
          <LoaderCircle className="h-4 w-4 animate-spin text-muted-foreground" />
        </div>
      )}
    </a>
  )
}

function StatusScreen({
  title,
  body,
  action,
}: {
  title: string
  body: string
  action?: ReactNode
}) {
  return (
    <div className="flex min-h-svh flex-col">
      <div className="mx-auto flex w-full max-w-lg flex-1 flex-col items-center justify-center gap-3 px-4 text-center">
        <LifeBuoy className="h-8 w-8 text-muted-foreground" />
        <h1 className="text-lg font-semibold">{title}</h1>
        <p className="text-sm text-muted-foreground">{body}</p>
        {action}
      </div>
      <PoweredByFooter />
    </div>
  )
}
