import { useRef, useState, type MouseEvent, type PointerEvent, type ReactNode } from "react"
import type { DeliveryState } from "@exp/domain-contract/message-delivery"
import { MESSAGE_DELIVERY_TEST_IDS } from "@exp/domain-contract/message-delivery"
import type { TranscriptRowLayout, TranscriptSender } from "@exp/domain-contract/transcript-grouping"
import { TRANSCRIPT_COPY, TRANSCRIPT_LAYOUT } from "@exp/domain-contract/transcript-grouping"
import { AttachmentFileLink } from "./attachment-thumb"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { Menu, type MenuEntry } from "./menu"
import { MessageStatusMark } from "./message-status-mark"
import { UserAvatar, type UserAvatarAgent, type UserAvatarUser } from "./user-avatar"

// #EXP-1736 (transcript-grouping.json): THE message bubble ×4, every chat a
// group chat. `side="out"` = the viewer's own (right, no avatar, no name);
// `side="in"` = anyone else's, its sender's name on the group's first bubble
// (a cross-session sender's ends in `›` and the whole bubble opens that
// session). `TranscriptRow` frames a row: the sender's avatar column, the
// gap above it. Before EXP-1736:
// EXP-1245: the run OWNER's own message in the Run face's chat of turns:
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

const SessionIcon = conceptIcon(`coding-assistant`)
const MoreIcon = conceptIcon(`ui-more`)

/** #FEED-237: a touch long-press opens the bubble's menu after this long. */
const LONG_PRESS_MS = 500

/** The bubble's width cap: phones `maxWidth.phone`, md+ `maxWidth.wide`
 *  (Tailwind needs the literal classes). */
const BUBBLE_WIDTH = `max-w-[78%] md:max-w-[62%]`

export function MessageBubble({
  side = `out`,
  text,
  renderText,
  children,
  imageSrc,
  files,
  author,
  onPress,
  time,
  caption,
  action,
  relay = null,
  status = null,
  menu,
  variant = `chat`,
  className,
}: {
  /** `out` = the viewer's own (right), `in` = anyone else's (left). */
  side?: `in` | `out`
  text?: string
  /** The app's text renderer (links, pills); plain pre-wrapped prose without. */
  renderText?: (text: string) => ReactNode
  /** Rich content in place of `text` (the session feed's folded prose). */
  children?: ReactNode
  /** The first image the message carried, drawn as a thumb. */
  imageSrc?: string | null
  /** Wave D: the files the message carried, as file chips. */
  files?: readonly { name: string; url: string }[]
  /** `in` only: the sender's name line (the group's first bubble). */
  author?: string | null
  /** `in` only: a cross-session sender: the whole bubble opens it.
   *  @deprecated #EXP-1772: a relay names its session ABOVE the bubble
   *  (`relay`), the bubble itself is never a link. */
  onPress?: () => void
  /** #EXP-1772 `in` only: a RELAY (another session's message, a child's
   *  report): its session's name + ` ›` ABOVE the bubble, the one press
   *  that opens it (`onPress` absent = a plain label). */
  relay?: { name: string; onPress?: () => void } | null
  /** The bubble's `HH:mm`, bottom-right inside. */
  time?: string | null
  /** Under the bubble (`queued`). */
  caption?: string | null
  /** EXP-1317: a trailing control on the caption line (a queued row's X). */
  action?: ReactNode
  /** #FEED-237 `out` only: the delivery mark beside the time
   *  (message-delivery.json); null = none. */
  status?: DeliveryState | null
  /** #FEED-237: the bubble's menu (right-click, the hover `…`, a touch
   *  long-press; a `failed` mark opens it too). Empty/absent = none. */
  menu?: readonly MenuEntry[]
  /** @deprecated #EXP-1736: one bubble everywhere; kept for old call sites. */
  variant?: `transcript` | `chat`
  className?: string
}) {
  void variant
  const out = side === `out`
  const [menuOpen, setMenuOpen] = useState(false)
  const pressTimer = useRef<number | null>(null)
  const hasMenu = Boolean(menu && menu.length > 0)
  const mark = out && status ? status : null
  const clearPress = () => {
    if (pressTimer.current !== null) window.clearTimeout(pressTimer.current)
    pressTimer.current = null
  }
  const menuGestures = hasMenu
    ? {
        onContextMenu: (event: MouseEvent) => {
          event.preventDefault()
          setMenuOpen(true)
        },
        onPointerDown: (event: PointerEvent) => {
          if (event.pointerType !== `touch`) return
          clearPress()
          pressTimer.current = window.setTimeout(() => setMenuOpen(true), LONG_PRESS_MS)
        },
        onPointerUp: clearPress,
        onPointerCancel: clearPress,
        onPointerLeave: clearPress,
      }
    : {}
  const name = !out && author && !relay ? author : null
  const inner = (
    <>
      {name ? (
        <div data-slot="message-bubble-author" className="mb-0.5 truncate text-xs font-medium text-muted-foreground">
          {name}
        </div>
      ) : null}
      {children ?? (text !== undefined ? renderText ? renderText(text) : <p className="whitespace-pre-wrap">{text}</p> : null)}
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
      {time || mark ? (
        <div data-slot="user-message-footer" className="mt-1 flex items-center justify-end gap-1">
          {time ? (
            <span data-slot="user-message-time" className="text-[0.6875rem] text-muted-foreground tabular-nums">
              {time}
            </span>
          ) : null}
          {mark === `failed` && hasMenu ? (
            <button
              type="button"
              className="inline-flex rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              onClick={() => setMenuOpen(true)}
            >
              <MessageStatusMark state={mark} />
            </button>
          ) : mark ? (
            <MessageStatusMark state={mark} />
          ) : null}
          {hasMenu ? (
            <Menu
              entries={menu}
              open={menuOpen}
              onOpenChange={setMenuOpen}
              align="end"
              data-testid={MESSAGE_DELIVERY_TEST_IDS.menu}
              trigger={
                <button
                  type="button"
                  aria-label="Message actions"
                  data-slot="user-message-more"
                  className="absolute top-1/2 -left-7 inline-flex size-6 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground opacity-0 outline-none transition-opacity group-hover/bubble:opacity-100 hover:text-foreground focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring/50 [@media(hover:none)]:hidden"
                >
                  <MoreIcon className="size-3.5" aria-hidden />
                </button>
              }
            />
          ) : null}
        </div>
      ) : null}
    </>
  )
  const bubbleClass = cn(
    `min-w-0 rounded-2xl border px-3.5 py-2.5 text-sm`,
    BUBBLE_WIDTH,
    out
      ? `border-glass-stroke-strong bg-glass-active text-foreground/90`
      : `border-glass-stroke-card bg-glass-card-opaque text-foreground`
  )
  return (
    <div
      data-testid={out ? `user-message-bubble` : `chat-bubble`}
      data-side={side}
      className={cn(`flex w-full flex-col gap-1`, out ? `items-end` : `items-start`, className)}
    >
      {!out && relay ? <RelayName name={relay.name} onPress={relay.onPress} /> : null}
      {!out && onPress && !relay ? (
        <div
          role="button"
          tabIndex={0}
          aria-label={name ?? undefined}
          data-slot="message-bubble-link"
          className={cn(bubbleClass, `cursor-pointer text-left outline-none hover:border-glass-stroke-strong focus-visible:border-glass-stroke-strong`)}
          onClick={(event) => {
            // A link inside the text keeps its own press.
            if ((event.target as HTMLElement).closest(`a,button`)) return
            onPress()
          }}
          onKeyDown={(event) => {
            if (event.target !== event.currentTarget) return
            if (event.key === `Enter` || event.key === ` `) {
              event.preventDefault()
              onPress()
            }
          }}
        >
          {inner}
        </div>
      ) : (
        <div className={cn(bubbleClass, hasMenu && `group/bubble relative`)} {...menuGestures}>
          {inner}
        </div>
      )}
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

/** #EXP-1772: a relay's session name over its bubble, ` ›` right of it;
 *  the name line's size and colour, the only press. */
function RelayName({ name, onPress }: { name: string; onPress?: () => void }) {
  const label = `${name} ${TRANSCRIPT_COPY.linkChevron}`
  const className = `max-w-full truncate px-1 text-xs font-medium text-muted-foreground`
  if (!onPress) {
    return (
      <div data-slot="message-bubble-relay" className={className}>
        {label}
      </div>
    )
  }
  return (
    <button
      type="button"
      data-slot="message-bubble-relay"
      onClick={onPress}
      className={cn(className, `rounded-sm text-left outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50`)}
    >
      {label}
    </button>
  )
}

/** The viewer's own bubble (`MessageBubble side="out"`). */
export function UserMessageBubble(props: Omit<Parameters<typeof MessageBubble>[0], `side` | `author` | `onPress`>) {
  return <MessageBubble {...props} side="out" />
}

/** A sender's name line: a cross-session sender's ends in `›`. */
export function transcriptSenderName(sender: TranscriptSender | null | undefined): string | null {
  if (!sender) return null
  return sender.link ? `${sender.name} ${TRANSCRIPT_COPY.linkChevron}` : sender.name
}

/** THE Avatar of a sender at its default 32: a member's initials in their
 *  hue, an agent's mark, a session's glyph on the muted disc. */
export function SenderAvatar({
  sender,
  user = null,
  agent = null,
}: {
  sender: TranscriptSender
  user?: UserAvatarUser | null
  agent?: UserAvatarAgent | null
}) {
  if (sender.kind === `member`) return <UserAvatar user={user ?? { id: sender.id, name: sender.name }} size={32} />
  if (sender.kind === `agent` && agent) return <UserAvatar user={null} agent={agent} size={32} />
  return (
    <span
      data-slot="session-avatar"
      className="relative flex size-8 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground"
    >
      <SessionIcon className="size-4" />
    </span>
  )
}

/** One transcript row: the gap above it, then `own` right, `full` across,
 *  `other` under its sender's avatar column (the avatar on the group's
 *  first row, top-aligned; the same space below it). */
export function TranscriptRow({
  layout,
  avatar,
  children,
  className,
}: {
  layout: Pick<TranscriptRowLayout, `side` | `avatar` | `gap`> | null | undefined
  avatar?: ReactNode
  children: ReactNode
  className?: string
}) {
  const style = layout && layout.gap > 0 ? { paddingTop: layout.gap } : undefined
  if (!layout || layout.side !== `other`) {
    return (
      <div data-slot="transcript-row" data-side={layout?.side ?? `full`} className={cn(`flex min-w-0 flex-col`, className)} style={style}>
        {children}
      </div>
    )
  }
  return (
    <div
      data-slot="transcript-row"
      data-side="other"
      className={cn(`flex min-w-0 items-start`, className)}
      style={{ ...style, gap: TRANSCRIPT_LAYOUT.avatarGap }}
    >
      <div className="flex size-8 shrink-0 items-start" aria-hidden={layout.avatar ? undefined : true}>
        {layout.avatar ? avatar : null}
      </div>
      {/* #EXP-1789: `items-start` sizes a child to its content, so a row
          whose content cannot wrap (a long path, an opened script) would run
          past the column: every child is capped at the column's width. */}
      <div className="flex min-w-0 flex-1 flex-col items-start *:max-w-full">{children}</div>
    </div>
  )
}
