import type { ReactNode } from "react"
import { AttachmentFileLink } from "./attachment-thumb"
import { cn } from "./cn"

// EXP-1245: the run OWNER's own message in the Run face's thread of turns:
// right-aligned, at most 70% of the column, the neutral glass bubble the
// transcript's human turn wears, an optional image thumb under the text and
// a muted caption under the bubble (`<name> · <time> · from <device>`).

/** `21:40`: local hours and minutes, both two-digit. */
export function userMessageTime(at: number | Date): string {
  const date = typeof at === `number` ? new Date(at) : at
  if (Number.isNaN(date.getTime())) return ``
  const pad = (value: number) => String(value).padStart(2, `0`)
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`
}

/** The caption under the bubble; a missing part drops with its separator. */
export function userMessageCaption(input: {
  name?: string | null
  at?: number | Date | null
  device?: string | null
}): string {
  const parts: string[] = []
  const name = input.name?.trim()
  if (name) parts.push(name)
  if (input.at !== null && input.at !== undefined) {
    const time = userMessageTime(input.at)
    if (time) parts.push(time)
  }
  const device = input.device?.trim()
  if (device) parts.push(`from ${device}`)
  return parts.join(` · `)
}

export function UserMessageBubble({
  text,
  renderText,
  imageSrc,
  files,
  caption,
  action,
  time,
  variant = `transcript`,
  className,
}: {
  /** Thread 58cb32df (chats.json TRANSCRIPT): `chat` = the phones' chat
   *  bubble, at most 80% of the column with its `time` bottom-right inside. */
  variant?: `transcript` | `chat`
  /** The `chat` bubble's `HH:mm`. */
  time?: string | null
  text: string
  /** The app's text renderer (links, pills); plain pre-wrapped prose without. */
  renderText?: (text: string) => ReactNode
  /** The first image the message carried, drawn as a thumb. */
  imageSrc?: string | null
  /** Wave D: the files the message carried, as the comment thread's file
   *  chips (glyph + name, opening the attachment). */
  files?: readonly { name: string; url: string }[]
  caption?: string | null
  /** EXP-1317: a trailing control on the caption line (a queued row's X). */
  action?: ReactNode
  className?: string
}) {
  return (
    <div
      data-testid="user-message-bubble"
      className={cn(`flex flex-col items-end gap-1`, className)}
    >
      <div
        className={cn(
          `min-w-0 border border-glass-stroke-strong bg-glass-active text-sm text-foreground/90`,
          variant === `chat` ? `max-w-[80%] rounded-2xl px-3.5 py-2.5` : `max-w-[70%] rounded-xl px-3 py-2`
        )}
      >
        {renderText ? renderText(text) : <p className="whitespace-pre-wrap">{text}</p>}
        {imageSrc ? (
          <img
            src={imageSrc}
            alt=""
            loading="lazy"
            data-slot="user-message-thumb"
            className="mt-2 h-20 w-auto rounded-md border border-glass-stroke-card object-cover"
          />
        ) : null}
        {files && files.length > 0 ? (
          <div className="mt-2 flex flex-wrap gap-2">
            {files.map((file) => (
              <AttachmentFileLink key={file.url} href={file.url} name={file.name} />
            ))}
          </div>
        ) : null}
        {variant === `chat` && time ? (
          <div data-slot="user-message-time" className="mt-1 text-right text-[0.6875rem] text-muted-foreground tabular-nums">
            {time}
          </div>
        ) : null}
      </div>
      {caption || action ? (
        <div className="flex items-center gap-1">
          {caption ? (
            <div data-slot="user-message-caption" className="pr-1 text-[0.6875rem] text-muted-foreground">
              {caption}
            </div>
          ) : null}
          {action}
        </div>
      ) : null}
    </div>
  )
}
