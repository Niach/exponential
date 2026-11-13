import { CHATS_COPY } from "@exp/domain-contract/chats"
import {
  Button,
  Badge,
  conceptIcon,
  MENU_SURFACE_CLASS,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@exp/ui"
import { InboxView } from "@/components/inbox/inbox-view"
import { setInboxPopoverOpen, useInboxPopoverOpen } from "@/hooks/use-inbox-popover"
import { markAllNotificationsRead, useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { cn } from "@/lib/utils"

// #FEED-191 (owner-approved draft): the WIDE Inbox: an icon in the sidebar's
// header row, (#FEED-232) THE count badge of the active team's unread
// notifications on its corner (muted), opening a menu-surface popover over
// the page: the `Inbox` caption with the mark-all-read glyph, then THE inbox's
// groups (`InboxView compact`), scrolling past `max-h`. A row marks read,
// navigates by its target and closes the popover. Phones keep the pushed page.

const NavInboxIcon = conceptIcon(`nav-inbox`)
const MarkReadIcon = conceptIcon(`notification-mark-read`)

export function InboxPopover({ teamId }: { teamId: string }) {
  const open = useInboxPopoverOpen()
  const unread = useUnreadNotificationCount(teamId)
  // The popover lists every team's rows; Mark all read follows the list.
  const unreadAnywhere = useUnreadNotificationCount()
  return (
    <Popover open={open} onOpenChange={setInboxPopoverOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <Button
              variant={open ? `secondary` : `ghost`}
              size="icon"
              className="relative size-8 shrink-0 text-muted-foreground hover:text-foreground"
              aria-label={CHATS_COPY.inbox}
              data-testid="nav-inbox"
            >
              <NavInboxIcon className="size-4" />
              <Badge
                count={unread}
                tone="muted"
                data-testid="nav-inbox-dot"
                className="absolute -top-1 -right-1"
              />
            </Button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent>{CHATS_COPY.inbox}</TooltipContent>
      </Tooltip>
      <PopoverContent
        align="start"
        sideOffset={6}
        data-testid="inbox-popover"
        className={cn(MENU_SURFACE_CLASS, `w-[24rem] gap-0 p-1`)}
      >
        <div className="flex h-8 items-center justify-between px-2">
          <span className="text-xs font-medium text-muted-foreground">{CHATS_COPY.inbox}</span>
          {unreadAnywhere > 0 && (
            <Button
              variant="ghost"
              size="icon"
              className="size-6 text-muted-foreground"
              aria-label="Mark all read"
              title="Mark all read"
              onClick={() => markAllNotificationsRead()}
            >
              <MarkReadIcon className="size-3.5" />
            </Button>
          )}
        </div>
        <div className="max-h-[24rem] overflow-y-auto overscroll-contain">
          <InboxView compact onRowOpen={() => setInboxPopoverOpen(false)} />
        </div>
      </PopoverContent>
    </Popover>
  )
}
