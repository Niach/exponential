import type { ReactNode } from "react"
import {
  SESSION_SCREEN_BESIDE,
  SESSION_SCREEN_COPY,
  SESSION_SCREEN_TEST_IDS,
  type SessionScreen,
  type SessionScreenTab,
} from "@exp/domain-contract/session-screen"
import {
  Button,
  conceptIcon,
  MOBILE_DETAIL_SCREEN_CLASS,
  MobileDetailHeader,
  PhoneHeaderTitle,
  SegmentedControl,
  useIsMobile,
} from "@exp/ui"
import { HEADER_BUTTON_CLASS } from "@/components/team/mobile-detail-header"
import { BackStackButton } from "@/components/chat/session-body"
import { cn } from "@/lib/utils"

// #EXP-1732 (fixtures/session-screen.json, its `$comment` = the spec): THE
// session screen's frame ×web, the same for every kind (a chat, an agent's
// or the Assistant's conversation, an app's editor session). It draws:
//
//   HEADER   the lead avatar, the title over ONE subtitle line, then the
//            tabs (#EXP-1762: ONE SegmentedControl `session-tabs` ON the
//            title row on wide, each label followed by its muted count;
//            phones keep it under the header, scrolling sideways), the
//            trailing slot (merge pill / Publish / call), THE gear (agent,
//            Assistant, app) and the `…`.
//   BODY     `sessionScreen().layout`: `beside` = the chat pane (5fr, min
//            360px) : the tab pane (7fr; the active tab's body or the
//            opened item over it), a hairline between; `chat` = the chat
//            full width; phones: `tab` (the tab's body) or `item` (the
//            opened item as a PUSHED PAGE: its own header, `‹` + the
//            item's title, replaces the screen header AND the tabs).
//
// The hosts (`chat-screen.tsx`, `apps/app-editor.tsx`) own the bodies and
// the route search; nothing here queries.

const NavSettingsIcon = conceptIcon(`nav-settings`)
const UiBackIcon = conceptIcon(`ui-back`)

export interface SessionScreenFrameProps {
  /** `sessionScreen(...)` for this screen. */
  screen: SessionScreen
  /** The root's test id (`chat-page`, the app editor's). */
  testId: string
  /** The lead avatar (the chat's lead disc, the agent's disc, the brand
   *  mark, an app's BoardGlyph). */
  avatar: ReactNode
  title: string
  /** After the title (the private lock, an app's version pill). */
  titleTrailing?: ReactNode
  /** `sessionScreenSubtitle(...)`; blank = no line. */
  subtitle: string
  /** #EXP-1729: the subtitle's tone class (the app editor's `editorHeader`
   *  tone); absent = muted. */
  subtitleClass?: string
  /** Before the gear: the merge pill, an app's Publish, the call button. */
  trailing?: ReactNode
  /** THE gear (only when `screen.gear`). */
  onGear?: () => void
  /** THE `…` (the chat / agent / app menu); gets the header button class. */
  menu?: (className: string) => ReactNode
  /** Phones: the header's back (to the list). */
  onBack?: () => void
  /** Wide: the inter-session back stack's `‹` (absent = none). */
  onPop?: () => void
  onTab: (tab: SessionScreenTab) => void
  /** THE chat body (the transcript, the Dock, the composer). */
  chat: ReactNode
  /** The active tab's body (unused while the active tab is chat). */
  pane: ReactNode
  /** The opened item's body (an issue, a session, a plan, settings). */
  item?: ReactNode
  /** The opened item's `‹` back (to the tab it came from). */
  onCloseItem?: () => void
  /** Phones: the opened item's title (its pushed page's header). */
  itemTitle?: string
  /** Over the body (the voice call card). */
  overlay?: ReactNode
}

/** THE tab strip: the existing SegmentedControl, each label followed by its
 *  count as a muted number (none for null). */
export function SessionScreenTabs({
  screen,
  onTab,
  phone,
}: {
  screen: SessionScreen
  onTab: (tab: SessionScreenTab) => void
  phone: boolean
}) {
  return (
    <div
      data-testid={SESSION_SCREEN_TEST_IDS.tabs}
      className={cn(`min-w-0`, phone && `-mx-3 overflow-x-auto px-3 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden`)}
    >
      <SegmentedControl
        value={screen.active}
        onValueChange={onTab}
        className={phone ? `w-max` : undefined}
        options={screen.tabs.map((tab) => ({
          value: tab.id,
          label: (
            <span className="flex items-center gap-1.5 whitespace-nowrap" data-tab={tab.id}>
              {tab.label}
              {tab.count !== null && (
                <span data-slot="session-tab-count" className="text-muted-foreground tabular-nums">
                  {tab.count}
                </span>
              )}
            </span>
          ),
        }))}
      />
    </div>
  )
}

/** THE gear: the existing ghost icon button, concept `nav-settings`. */
export function SessionScreenGear({ onClick, className }: { onClick: () => void; className?: string }) {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={SESSION_SCREEN_COPY.settings}
      title={SESSION_SCREEN_COPY.settings}
      data-testid={SESSION_SCREEN_TEST_IDS.gear}
      onClick={onClick}
      className={cn(`shrink-0`, className)}
    >
      <NavSettingsIcon className="size-4" />
    </Button>
  )
}

/** An opened item's `‹` back over its body. */
function ItemBack({ onBack }: { onBack: () => void }) {
  return (
    <div className="flex shrink-0 items-center px-2 pt-2">
      <Button
        variant="ghost"
        size="sm"
        className="gap-1 text-muted-foreground"
        data-testid="session-item-back"
        onClick={onBack}
      >
        <UiBackIcon className="size-4" />
        {SESSION_SCREEN_COPY.back}
      </Button>
    </div>
  )
}

export function SessionScreenFrame({
  screen,
  testId,
  avatar,
  title,
  titleTrailing,
  subtitle,
  subtitleClass = `text-muted-foreground`,
  trailing,
  onGear,
  menu,
  onBack,
  onPop,
  onTab,
  chat,
  pane,
  item,
  onCloseItem,
  itemTitle,
  overlay,
}: SessionScreenFrameProps) {
  const phone = useIsMobile()
  const tabs = <SessionScreenTabs screen={screen} onTab={onTab} phone={phone} />
  const gear = screen.gear && onGear ? (
    <SessionScreenGear onClick={onGear} className={phone ? HEADER_BUTTON_CLASS : `text-muted-foreground`} />
  ) : null

  const opened = item ? (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col" data-testid="session-item">
      {onCloseItem && <ItemBack onBack={onCloseItem} />}
      <div className="flex min-h-0 flex-1 flex-col">{item}</div>
    </div>
  ) : null
  const paneBody = opened ?? <div className="flex min-h-0 min-w-0 flex-1 flex-col">{pane}</div>

  let body: ReactNode
  if (screen.layout === `beside`) {
    body = (
      <div
        data-testid={SESSION_SCREEN_TEST_IDS.beside}
        className="grid min-h-0 flex-1"
        style={{
          gridTemplateColumns: `minmax(${SESSION_SCREEN_BESIDE.chatMinWidth}px, ${SESSION_SCREEN_BESIDE.chat}fr) ${SESSION_SCREEN_BESIDE.pane}fr`,
        }}
      >
        <div className="flex min-h-0 min-w-0 flex-col border-r border-border">{chat}</div>
        <div data-testid={SESSION_SCREEN_TEST_IDS.pane} className="flex min-h-0 min-w-0 flex-col">
          {paneBody}
        </div>
      </div>
    )
  } else if (screen.layout === `item`) {
    body = opened ?? paneBody
  } else if (screen.layout === `tab`) {
    body = paneBody
  } else {
    body = <div className="flex min-h-0 min-w-0 flex-1 flex-col">{chat}</div>
  }

  // session-screen.json PHONE ITEM: an opened item is a pushed page; its
  // own header (‹ + the item's title) replaces the screen header and tabs.
  if (phone && screen.layout === `item` && item) {
    return (
      <div className={MOBILE_DETAIL_SCREEN_CLASS} data-testid={testId} data-layout={screen.layout}>
        <MobileDetailHeader
          title={<span data-testid="session-item-title">{itemTitle ?? ``}</span>}
          onBack={onCloseItem}
          backLabel={SESSION_SCREEN_COPY.back}
        />
        {overlay}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col" data-testid="session-item">
          {item}
        </div>
      </div>
    )
  }

  if (phone) {
    return (
      <div className={MOBILE_DETAIL_SCREEN_CLASS} data-testid={testId} data-layout={screen.layout}>
        <MobileDetailHeader
          title={
            <PhoneHeaderTitle
              disc={avatar}
              title={
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className="truncate" data-testid="chat-title">
                    {title}
                  </span>
                  {titleTrailing}
                </span>
              }
              subtitle={subtitle ? <span className={subtitleClass}>{subtitle}</span> : null}
            />
          }
          onBack={onBack}
          menu={
            <span className="flex shrink-0 items-center gap-1">
              {trailing}
              {gear}
              {menu?.(HEADER_BUTTON_CLASS)}
            </span>
          }
          below={<div className="px-3 pb-2">{tabs}</div>}
        />
        {overlay}
        <div className="flex min-h-0 flex-1 flex-col">{body}</div>
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid={testId} data-layout={screen.layout}>
      <div className="shrink-0 border-b border-border px-6 py-3">
        <div className="flex items-center gap-3">
          <BackStackButton onPop={onPop} className="-ml-2" />
          {avatar}
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-2">
              <h1 data-testid="chat-title" className="min-w-0 truncate text-lg font-semibold tracking-tight">
                {title}
              </h1>
              {titleTrailing}
            </div>
            {subtitle && (
              <p data-testid="chat-header-subtitle" className={cn(`truncate text-xs`, subtitleClass)}>
                {subtitle}
              </p>
            )}
          </div>
          {/* #EXP-1762: ONE header line on wide: the tabs sit on the title
              row, before the merge pill, the gear and the `…`. */}
          <div className="shrink-0">{tabs}</div>
          {trailing}
          {gear}
          {menu?.(`shrink-0 text-muted-foreground`)}
        </div>
      </div>
      {overlay}
      {body}
    </div>
  )
}
