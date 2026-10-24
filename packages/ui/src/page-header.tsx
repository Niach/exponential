import type { ReactNode } from "react"
import { CHATS_COPY, CHATS_SHELL, CHATS_TEST_IDS } from "@exp/domain-contract/chats"

import { cn } from "./cn"
import { FabButton } from "./fab-chrome"
import { conceptIcon } from "./icons.generated"

// #EXP-1384 (fixture `chats.json` `shell.header`, owner 2026-10-24): THE
// phone page header on every top-level page (Board, Chats, Apps, Team,
// Devices, Agent): the large in-content title on the left and the round
// trailing circles `shell.header.buttons`, search then the INBOX wearing a
// sky dot while a notification of the active team is unread. Android
// `ui/components` `PageHeader` is the twin; iOS paints the large navigation
// title with the native toolbar circles (`PageHeaderToolbar`).

const SearchIcon = conceptIcon(`nav-search`)
const InboxIcon = conceptIcon(`nav-inbox`)

export function PageHeader({
  title,
  leading,
  trailing,
  inboxUnread = false,
  onSearch,
  onInbox,
  className,
}: {
  title: ReactNode
  /** Before the title (Devices' back control). */
  leading?: ReactNode
  /** Extra circles before search (rare). */
  trailing?: ReactNode
  inboxUnread?: boolean
  onSearch?: () => void
  onInbox?: () => void
  className?: string
}) {
  const circle = CHATS_SHELL.header.circle
  return (
    <div
      data-testid={CHATS_TEST_IDS.pageHeader}
      className={cn(`flex items-center gap-3`, className)}
    >
      {leading}
      <h1 className="min-w-0 flex-1 truncate text-[34px] font-bold leading-tight tracking-tight">
        {title}
      </h1>
      {trailing}
      <FabButton
        aria-label="Search"
        data-testid={CHATS_TEST_IDS.searchButton}
        emphasis="primary"
        style={{ width: circle, height: circle }}
        onClick={onSearch}
      >
        <SearchIcon className="size-5" />
      </FabButton>
      <FabButton
        aria-label={CHATS_COPY.inbox}
        data-testid={CHATS_TEST_IDS.inboxButton}
        emphasis="primary"
        className="relative"
        style={{ width: circle, height: circle }}
        onClick={onInbox}
      >
        <InboxIcon className="size-5" />
        {inboxUnread && (
          <span
            aria-hidden
            data-slot="inbox-dot"
            className="absolute right-2 top-2 size-2.5 rounded-full bg-sky-500 ring-2 ring-background"
          />
        )}
      </FabButton>
    </div>
  )
}
