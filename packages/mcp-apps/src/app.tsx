import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { Button, ColorModeProvider, GLASS_CARD_CLASS, ListEmpty, Skeleton, cn, type ColorMode } from "@exp/ui"
import { actionsFromBridge, McpActionsProvider } from "./actions"
import { AppBridge, type DisplayMode, type HostContext, type ToolResult } from "./bridge"
import type { DevicesView } from "./devices-view"
import { applyColorMode, applyHostContext } from "./host-theme"
import type { IssueDetailView } from "./issue-detail-view"
import type { InboxView } from "./inbox-view"
import type { IssueListView } from "./issue-list-view"
import {
  MCP_APP_VIEW_TOOL,
  decodeToolResult,
  type IssueDetail,
  type IssueRow,
  type DeviceRow,
  type NotificationRow,
} from "./model"
import { EXPIRED_LINE, useViewLease } from "./lease"
import { defaultDisplayMode, listRows, screenForCall, type ListPage } from "./view-screen"

/** #NEW-24: the views that have a document of their own (`<view>.html`,
 *  vite.config.ts). The session views went with EXP-1291. */
export type AppView = `issues` | `inbox` | `devices`

/** The components ONE document draws with. Each entry (src/entries/) passes
 *  only its own, so a document bundles nothing another view needs: the
 *  devices document carries no markdown, the lists no account sign-in. */
export interface AppViews {
  IssueListView?: typeof IssueListView
  InboxView?: typeof InboxView
  DevicesView?: typeof DevicesView
  /** The issue a list row or a one-issue op opens in place. */
  IssueDetailView?: typeof IssueDetailView
}

// A list screen a detail can return to.
type ListScreen =
  | { kind: `issues`; issues: IssueRow[]; page: ListPage | null }
  | { kind: `inbox`; notifications: NotificationRow[]; page: ListPage | null }
  | { kind: `devices`; devices: DeviceRow[] }

type Screen =
  | { kind: `waiting` }
  | { kind: `none` }
  | { kind: `error`; message: string }
  | ListScreen
  | { kind: `issue`; issue: IssueDetail; back?: ListScreen }

function listScreen(view: AppView, rows: unknown[], page: ListPage | null): ListScreen {
  if (view === `inbox`) return { kind: `inbox`, notifications: rows as NotificationRow[], page }
  if (view === `devices`) return { kind: `devices`, devices: rows as DeviceRow[] }
  return { kind: `issues`, issues: rows as IssueRow[], page }
}

/** What the issue tool's answer decodes to. */
function issueScreen(result: ToolResult): Screen {
  const decoded = decodeToolResult<IssueDetail>(result)
  return decoded.kind === `error` ? decoded : { kind: `issue`, issue: decoded.data }
}

/** The list noun a view pages through (the view's own tool). */
const LIST_TOOL: Partial<Record<AppView, string>> = {
  issues: `exponential_issues`,
  inbox: `exponential_notifications`,
}

export function App({ view, views }: { view: AppView; views: AppViews }) {
  const [screen, setScreen] = useState<Screen>({ kind: `waiting` })
  const [mode, setMode] = useState<DisplayMode>(`inline`)
  const [theme, setTheme] = useState<ColorMode>(`dark`)
  const [loadingMore, setLoadingMore] = useState(false)
  const bridge = useRef<AppBridge | null>(null)
  const root = useRef<HTMLDivElement>(null)
  // The call's own arguments (tool-input arrives before its result).
  const input = useRef<Record<string, unknown>>({})
  const available = useRef<readonly DisplayMode[]>([])
  // #NEW-24: read-only once the host's lease on the view expired (lease.ts).
  const lease = useViewLease<Screen>(view)
  const leaseRef = useRef(lease)
  leaseRef.current = lease

  useEffect(() => {
    const context = (next: HostContext) => {
      leaseRef.current.onContext(next)
      // #NEW-24: the host's theme, style variables and safe area.
      const named = applyHostContext(next)
      if (named) setTheme(named)
      if (next.availableDisplayModes) available.current = next.availableDisplayModes
      if (next.displayMode) setMode(next.displayMode)
    }
    const instance = new AppBridge({
      onToolInput: (args) => {
        input.current = args
        leaseRef.current.onToolInput()
      },
      onHostContext: context,
      onToolResult: (result) => {
        leaseRef.current.onToolResult()
        const op = typeof input.current.op === `string` ? input.current.op : undefined
        const call = screenForCall(view, op, result)
        if (call.kind === `none`) return setScreen({ kind: `none` })
        if (call.kind === `error`) return setScreen(call)
        if (call.kind === `list`) {
          setScreen(listScreen(view, call.rows, call.page))
          // #VAPP-292: a board the person asked to see opens full size (the
          // host's side pane or full page) when the host offers it.
          const wanted = defaultDisplayMode(view, op)
          if (wanted !== `inline` && available.current.includes(wanted)) {
            void instance.requestDisplayMode(wanted).then((set) => set && setMode(set))
          }
          return
        }
        if (call.detail) return setScreen({ kind: `issue`, issue: call.detail as IssueDetail })
        void open(call.id, undefined)
      },
    })
    bridge.current = instance
    instance
      .connect()
      .then(context)
      .catch(() => {
        // No host answered: the view stays on its waiting state.
      })
    return () => {
      instance.dispose()
      bridge.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view])

  // Exponential is dark on every client, so dark is the fallback: a host
  // that names a theme gets its own (#NEW-24, host-theme.ts).
  useLayoutEffect(() => {
    applyColorMode(theme)
  }, [theme])

  // Inline, the host sizes the frame to the content; full size, the frame is
  // the host's and the view scrolls inside it.
  useEffect(() => {
    const node = root.current
    if (!node || typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(() => {
      bridge.current?.sizeChanged(Math.ceil(node.getBoundingClientRect().height))
    })
    observer.observe(node)
    return () => observer.disconnect()
  }, [])

  // Every tools/call of the view goes through the lease: none once it
  // expired, and a refused first call expires it.
  const callTool = useMemo(
    () =>
      leaseRef.current.guard((name, args) =>
        bridge.current ? bridge.current.callTool(name, args) : Promise.reject(new Error(`No MCP Apps host.`))
      ),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    []
  )
  // The call's last screen, for the read-only state.
  useEffect(() => {
    if (screen.kind === `issues` || screen.kind === `inbox` || screen.kind === `devices` || screen.kind === `issue`) lease.keep(screen)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [screen])

  const openLink = (url: string) => bridge.current?.openLink(url)
  // Stable for the view's lifetime: views key effects on `call`.
  const actions = useMemo(
    () =>
      actionsFromBridge(callTool, (url) => bridge.current?.openLink(url)),
    [callTool]
  )
  const backTo = (back: ListScreen | undefined) =>
    back ? () => setScreen(back) : undefined

  // A list row opens its detail through the host (the detail tool), and the
  // detail's back arrow returns to the list it came from.
  async function open(id: string, back: ListScreen | undefined) {
    if (!bridge.current) return
    try {
      const next = issueScreen(await callTool(MCP_APP_VIEW_TOOL.issue, { id }))
      setScreen(next.kind === `issue` ? { ...next, back } : next)
    } catch (error) {
      // An expired view keeps the screen it has.
      if (leaseRef.current.isExpired()) return
      setScreen({
        kind: `error`,
        message: error instanceof Error ? error.message : String(error),
      })
    }
  }

  // The next page of the call's own list, appended in place.
  const more = async (current: Extract<ListScreen, { kind: `issues` | `inbox` }>) => {
    const tool = LIST_TOOL[view]
    const next = current.page?.nextOffset
    if (!tool || next == null || !bridge.current || loadingMore) return
    setLoadingMore(true)
    try {
      const args = { ...input.current, op: input.current.op ?? `list`, offset: next, limit: current.page?.limit }
      const page = listRows(await callTool(tool, args))
      if (!page) return
      setScreen((shown) => {
        if (shown.kind === `issues`) return { ...shown, issues: [...shown.issues, ...(page.rows as IssueRow[])], page: page.page }
        if (shown.kind === `inbox`) return { ...shown, notifications: [...shown.notifications, ...(page.rows as NotificationRow[])], page: page.page }
        return shown
      })
    } catch {
      // A refused page leaves the list as it is.
    } finally {
      setLoadingMore(false)
    }
  }

  // Read-only: the call's last screen when this page never drew one.
  const shown: Screen = lease.expired && screen.kind === `waiting` && lease.restored ? lease.restored : screen
  const full = mode === `fullscreen`
  const showMore = (current: Extract<ListScreen, { kind: `issues` | `inbox` }>) =>
    current.page?.nextOffset != null && (
      // The board list's own "Show more" (issue-list.tsx, REV-46).
      <Button
        variant="ghost"
        size="sm"
        className="w-full rounded-none text-xs text-muted-foreground"
        disabled={loadingMore}
        data-testid="view-show-more"
        onClick={() => void more(current)}
      >
        {`Show ${current.page.limit} more`}
      </Button>
    )

  return (
    <ColorModeProvider value={theme}>
    <McpActionsProvider value={actions}>
      <div
        ref={root}
        data-display-mode={mode}
        data-screen={shown.kind}
        data-expired={lease.expired || undefined}
        // Inline: the app's own card (GlassCard), the glass fill and hairline
        // over the host's ground. Full size: the app's page, edge to edge,
        // scrolling inside the host's pane.
        className={cn(
          `font-sans text-foreground antialiased`,
          shown.kind === `none`
            ? `hidden`
            : full
              ? `flex h-screen flex-col overflow-y-auto bg-background`
              : cn(GLASS_CARD_CLASS, `overflow-hidden`)
        )}
      >
        {lease.expired && <ListEmpty data-testid="view-expired">{EXPIRED_LINE}</ListEmpty>}
        <div className="contents" inert={lease.expired}>
        {shown.kind === `waiting` && !lease.expired && (
          <div className="flex flex-col gap-2 p-4">
            <Skeleton className="h-6 w-1/3" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        )}
        {shown.kind === `error` && <ListEmpty>{shown.message}</ListEmpty>}
        {shown.kind === `issues` && views.IssueListView && (
          <div className={cn(full && `mx-auto w-full max-w-5xl py-2`)}>
            <views.IssueListView
              issues={shown.issues}
              onOpen={(row) => void open(row.id, shown)}
            />
            {showMore(shown)}
          </div>
        )}
        {shown.kind === `inbox` && views.InboxView && (
          <div className={cn(full && `mx-auto w-full max-w-3xl py-2`)}>
            <views.InboxView
              notifications={shown.notifications}
              onOpen={(row) => row.issueId && void open(row.issueId, shown)}
            />
            {showMore(shown)}
          </div>
        )}
        {shown.kind === `issue` && views.IssueDetailView && (
          <div className={cn(full && `mx-auto w-full max-w-4xl py-2`)}>
            <views.IssueDetailView
              issue={shown.issue}
              onOpenLink={openLink}
              onBack={backTo(shown.back)}
            />
          </div>
        )}
        {shown.kind === `devices` && views.DevicesView && (
          <div className={cn(full && `mx-auto w-full max-w-5xl py-2`)}>
            <views.DevicesView devices={shown.devices} />
          </div>
        )}
        </div>
      </div>
    </McpActionsProvider>
    </ColorModeProvider>
  )
}
