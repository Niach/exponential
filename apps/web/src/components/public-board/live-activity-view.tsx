import { useEffect, useRef, useState } from "react"
import { Radio, TerminalSquare, Wrench } from "lucide-react"
import { trpc } from "@/lib/trpc-client"

// The public live-coding stream: a stripped, desktop-side-scrubbed feed of
// tool headlines + narration + the latest worktree diff, delivered over the
// steer relay's public activity channel. This is NEVER a terminal — the relay
// structurally cannot send PTY bytes to a public_viewer socket.

type ActivityEvent =
  | { kind: `narration`; text: string; at?: number }
  | { kind: `tool`; name: string; detail?: string; at?: number }
  | { kind: `diff`; diff: string; at?: number }

const MAX_FEED = 200

export function LiveActivityView({
  codingSessionId,
  deviceLabel,
}: {
  codingSessionId: string
  deviceLabel: string | null
}) {
  const [events, setEvents] = useState<
    Exclude<ActivityEvent, { kind: `diff` }>[]
  >([])
  const [diff, setDiff] = useState<string | null>(null)
  const [status, setStatus] = useState<
    `connecting` | `live` | `ended` | `unavailable`
  >(`connecting`)
  const feedRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    let socket: WebSocket | null = null
    let disposed = false

    const connect = async () => {
      try {
        const minted = await trpc.steer.mintPublicViewTicket.mutate(
          { codingSessionId },
          { context: { skipErrorToast: true } }
        )
        if (disposed) return
        if (`disabled` in minted) {
          setStatus(`unavailable`)
          return
        }
        socket = new WebSocket(minted.url)
        socket.onopen = () => {
          socket?.send(JSON.stringify({ t: `join` }))
          if (!disposed) setStatus(`live`)
        }
        socket.onmessage = (message) => {
          if (typeof message.data !== `string`) return
          let frame: { t?: string; event?: ActivityEvent; outcome?: string }
          try {
            frame = JSON.parse(message.data)
          } catch {
            return
          }
          if (frame.t === `activity` && frame.event) {
            const event = frame.event
            if (event.kind === `diff`) {
              setDiff(event.diff)
            } else if (event.kind === `narration` || event.kind === `tool`) {
              // Whitelist, not fall-through: the relay never sends the
              // member-only kinds (user_message/question) to public sockets,
              // but an unknown future kind must not render as fake narration.
              setEvents((prev) => [...prev.slice(-MAX_FEED + 1), event])
            }
          } else if (frame.t === `bye`) {
            setStatus(`ended`)
          }
        }
        socket.onclose = () => {
          if (!disposed) {
            setStatus((prev) => (prev === `live` ? `ended` : prev))
          }
        }
      } catch {
        if (!disposed) setStatus(`unavailable`)
      }
    }
    void connect()
    return () => {
      disposed = true
      socket?.close()
    }
  }, [codingSessionId])

  // Keep the feed pinned to the newest event.
  useEffect(() => {
    feedRef.current?.scrollTo({ top: feedRef.current.scrollHeight })
  }, [events])

  if (status === `unavailable`) return null

  return (
    <section className="space-y-3 rounded-md border border-border/60 p-4">
      <div className="flex items-center gap-2 text-sm font-medium">
        <Radio
          className={`h-4 w-4 ${status === `live` ? `animate-pulse text-red-500` : `text-muted-foreground`}`}
        />
        {status === `live`
          ? `Live: this issue is being coded right now`
          : status === `ended`
            ? `The coding session ended`
            : `Connecting to the live session…`}
        {deviceLabel && (
          <span className="text-xs font-normal text-muted-foreground">
            on {deviceLabel}
          </span>
        )}
      </div>

      {events.length > 0 && (
        <div
          ref={feedRef}
          className="max-h-64 space-y-1.5 overflow-y-auto text-sm"
        >
          {events.map((event, index) => (
            <div key={index} className="flex items-start gap-2">
              {event.kind === `tool` ? (
                <>
                  <Wrench className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0">
                    <span className="font-medium">{event.name}</span>
                    {event.detail && (
                      <span className="ml-1.5 break-all font-mono text-xs text-muted-foreground">
                        {event.detail}
                      </span>
                    )}
                  </span>
                </>
              ) : (
                <>
                  <TerminalSquare className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 whitespace-pre-wrap text-muted-foreground">
                    {event.text}
                  </span>
                </>
              )}
            </div>
          ))}
        </div>
      )}

      {diff && (
        <div className="overflow-x-auto rounded-md border border-border/60 bg-muted/30">
          <pre className="p-3 text-xs leading-relaxed">
            {diff.split(`\n`).map((line, index) => (
              <div
                key={index}
                className={
                  line.startsWith(`+`) && !line.startsWith(`+++`)
                    ? `text-green-500`
                    : line.startsWith(`-`) && !line.startsWith(`---`)
                      ? `text-red-500`
                      : line.startsWith(`@@`)
                        ? `text-blue-400`
                        : line.startsWith(`diff `) || line.startsWith(`+++`) || line.startsWith(`---`)
                          ? `font-semibold text-muted-foreground`
                          : undefined
                }
              >
                {line || ` `}
              </div>
            ))}
          </pre>
        </div>
      )}
    </section>
  )
}
