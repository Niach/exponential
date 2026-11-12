import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { Button, GLASS_CARD_CLASS, ListEmpty, Skeleton, cn } from "@exp/ui"
import { actionsFromBridge, McpActionsProvider } from "./actions"
import { AppBridge, type DisplayMode, type HostContext, type ToolResult } from "./bridge"
import { DevicesView } from "./devices-view"
import { IssueDetailView } from "./issue-detail-view"
import { InboxView } from "./inbox-view"
import { IssueListView } from "./issue-list-view"
import { RunView } from "./run-view"
import { RunsListView } from "./runs-list-view"
import {
  MCP_APP_VIEW_TOOL,
  decodeToolResult,
  type IssueDetail,
  type IssueRow,
  type DeviceRow,
  type McpAppView,
  type NotificationRow,
  type RunDetail,
} from "./model"
import { defaultDisplayMode, listRows, screenForCall, type ListPage } from "./view-screen"

// A list screen a detail can return to.
type ListScreen =
  | { kind: `issues`; issues: IssueRow[]; page: ListPage | null }
  | { kind: `runs`; runs: RunDetail[] }
  | { kind: `inbox`; notifications: NotificationRow[]; page: ListPage | null }
  | { kind: `devices`; devices: DeviceRow[] }

type Screen =
  | { kind: `waiting` }
  | { kind: `none` }
  | { kind: `error`; message: string }
  | ListScreen
  | { kind: `issue`; issue: IssueDetail; back?: ListScreen }
  | { kind: `run`; run: RunDetail; back?: ListScreen }

function listScreen(view: McpAppView, rows: unknown[], page: ListPage | null): ListScreen {
  if (view === `runs`) return { kind: `runs`, runs: rows as RunDetail[] }
  if (view === `inbox`) return { kind: `inbox`, notifications: rows as NotificationRow[], page }
  if (view === `devices`) return { kind: `devices`, devices: rows as DeviceRow[] }
  return { kind: `issues`, issues: rows as IssueRow[], page }
}

/** What a view's own tool result decodes to (a detail tool's answer, or a
 *  list answer when the op is unknown). */
export function screenFor(view: McpAppView, result: ToolResult): Screen {
  if (view === `issue`) {
    const decoded = decodeToolResult<IssueDetail>(result)
    return decoded.kind === `error` ? decoded : { kind: `issue`, issue: decoded.data }
  }
  if (view === `run`) {
    const decoded = decodeToolResult<RunDetail>(result)
    return decoded.kind === `error` ? decoded : { kind: `run`, run: decoded.data }
  }
  const decoded = decodeToolResult<unknown>(result)
  if (decoded.kind === `error`) return decoded
  const list = listRows(result)
  return listScreen(view, list?.rows ?? [], list?.page ?? null)
}

/** The list noun a view pages through (the view's own tool). */
const LIST_TOOL: Partial<Record<McpAppView, string>> = {
  issues: `exponential_issues`,
  inbox: `exponential_notifications`,
}

export function App({ view }: { view: McpAppView }) {
  const [screen, setScreen] = useState<Screen>({ kind: `waiting` })
  const [mode, setMode] = useState<DisplayMode>(`inline`)
  const [loadingMore, setLoadingMore] = useState(false)
  const bridge = useRef<AppBridge | null>(null)
  const root = useRef<HTMLDivElement>(null)
  // The call's own arguments (tool-input arrives before its result).
  const input = useRef<Record<string, unknown>>({})
  const available = useRef<readonly DisplayMode[]>([])

  useEffect(() => {
    const context = (next: HostContext) => {
      if (next.availableDisplayModes) available.current = next.availableDisplayModes
      if (next.displayMode) setMode(next.displayMode)
    }
    const instance = new AppBridge({
      onToolInput: (args) => {
        input.current = args
      },
      onHostContext: context,
      onToolResult: (result) => {
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
        void open(`issue`, call.id, undefined)
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

  // Exponential is dark on every client (the web forces `html.dark`), so the
  // views are too, whatever the host's own theme: they read as the app.
  useLayoutEffect(() => {
    document.documentElement.classList.add(`dark`)
  }, [])

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

  const openLink = (url: string) => bridge.current?.openLink(url)
  // Stable for the view's lifetime: views key effects on `call`.
  const actions = useMemo(
    () =>
      actionsFromBridge(
        (name, args) =>
          bridge.current
            ? bridge.current.callTool(name, args)
            : Promise.reject(new Error(`No MCP Apps host.`)),
        (url) => bridge.current?.openLink(url)
      ),
    []
  )
  const backTo = (back: ListScreen | undefined) =>
    back ? () => setScreen(back) : undefined

  // A list row opens its detail through the host (the detail tool), and the
  // detail's back arrow returns to the list it came from.
  async function open(detail: `issue` | `run`, id: string, back: ListScreen | undefined) {
    const instance = bridge.current
    if (!instance) return
    try {
      const result = await instance.callTool(MCP_APP_VIEW_TOOL[detail], { id })
      const next = screenFor(detail, result)
      setScreen(
        next.kind === `issue` || next.kind === `run` ? { ...next, back } : next
      )
    } catch (error) {
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
      const page = listRows(await bridge.current.callTool(tool, args))
      if (!page) return
      setScreen((shown) => {
        if (shown.kind === `issues`) return { ...shown, issues: [...shown.issues, ...(page.rows as IssueRow[])], page: page.page }
        if (shown.kind === `inbox`) return { ...shown, notifications: [...shown.notifications, ...(page.rows as NotificationRow[])], page: page.page }
        return shown
      })
    } finally {
      setLoadingMore(false)
    }
  }

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
    <McpActionsProvider value={actions}>
      <div
        ref={root}
        data-display-mode={mode}
        data-screen={screen.kind}
        // Inline: the app's own card (GlassCard), the glass fill and hairline
        // over the host's ground. Full size: the app's page, edge to edge,
        // scrolling inside the host's pane.
        className={cn(
          `font-sans text-foreground antialiased`,
          screen.kind === `none`
            ? `hidden`
            : full
              ? `flex h-screen flex-col overflow-y-auto bg-background`
              : cn(GLASS_CARD_CLASS, `overflow-hidden`)
        )}
      >
        {screen.kind === `waiting` && (
          <div className="flex flex-col gap-2 p-4">
            <Skeleton className="h-6 w-1/3" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        )}
        {screen.kind === `error` && <ListEmpty>{screen.message}</ListEmpty>}
        {screen.kind === `issues` && (
          <div className={cn(full && `mx-auto w-full max-w-5xl py-2`)}>
            <IssueListView
              issues={screen.issues}
              onOpen={(row) => void open(`issue`, row.id, screen)}
            />
            {showMore(screen)}
          </div>
        )}
        {screen.kind === `runs` && (
          <RunsListView
            runs={screen.runs}
            onOpen={(run) => void open(`run`, run.id, screen)}
          />
        )}
        {screen.kind === `inbox` && (
          <div className={cn(full && `mx-auto w-full max-w-3xl py-2`)}>
            <InboxView
              notifications={screen.notifications}
              onOpen={(row) => row.issueId && void open(`issue`, row.issueId, screen)}
            />
            {showMore(screen)}
          </div>
        )}
        {screen.kind === `issue` && (
          <div className={cn(full && `mx-auto w-full max-w-4xl py-2`)}>
            <IssueDetailView
              issue={screen.issue}
              onOpenLink={openLink}
              onBack={backTo(screen.back)}
            />
          </div>
        )}
        {screen.kind === `run` && (
          <RunView run={screen.run} onOpenLink={openLink} onBack={backTo(screen.back)} />
        )}
        {screen.kind === `devices` && (
          <div className={cn(full && `mx-auto w-full max-w-5xl py-2`)}>
            <DevicesView devices={screen.devices} />
          </div>
        )}
      </div>
    </McpActionsProvider>
  )
}
