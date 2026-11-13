import type { DeliveryState } from "@exp/domain-contract/message-delivery"
import {
  MESSAGE_DELIVERY_COPY,
  MESSAGE_DELIVERY_MARKS,
  MESSAGE_DELIVERY_TEST_IDS,
} from "@exp/domain-contract/message-delivery"
import type { ComponentType, SVGProps } from "react"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"

// #FEED-237 (message-delivery.json): THE delivery mark beside the viewer's
// own bubble's `HH:mm` ×4 (genUI `MessageStatus`): the clock (not on the
// device yet), one check (the device holds it), two checks (processed into
// the session, the read blue = the theme's `info`, #3b82f6), the red mark
// (the send failed). 14 px.

const GLYPHS: Record<DeliveryState, ComponentType<SVGProps<SVGSVGElement>>> = {
  sending: conceptIcon(MESSAGE_DELIVERY_MARKS.sending.icon as Parameters<typeof conceptIcon>[0]),
  sent: conceptIcon(MESSAGE_DELIVERY_MARKS.sent.icon as Parameters<typeof conceptIcon>[0]),
  read: conceptIcon(MESSAGE_DELIVERY_MARKS.read.icon as Parameters<typeof conceptIcon>[0]),
  failed: conceptIcon(MESSAGE_DELIVERY_MARKS.failed.icon as Parameters<typeof conceptIcon>[0]),
}

const TONE_CLASS: Record<string, string> = {
  muted: `text-muted-foreground`,
  info: `text-blue-500`,
  danger: `text-destructive`,
}

export function MessageStatusMark({ state, className }: { state: DeliveryState; className?: string }) {
  const Glyph = GLYPHS[state]
  const label = MESSAGE_DELIVERY_COPY.status[state]
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      data-testid={MESSAGE_DELIVERY_TEST_IDS.status}
      data-state={state}
      className={cn(`inline-flex size-3.5 shrink-0 items-center justify-center`, TONE_CLASS[MESSAGE_DELIVERY_MARKS[state].tone], className)}
    >
      <Glyph className="size-3.5" aria-hidden />
    </span>
  )
}
