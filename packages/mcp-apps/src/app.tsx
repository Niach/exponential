import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { GLASS_CARD_CLASS, ListEmpty, Skeleton, cn } from "@exp/ui"
import { actionsFromBridge, McpActionsProvider } from "./actions"
import { AppBridge, type ToolResult } from "./bridge"
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

// A list screen a detail can return to.
type ListScreen =
  | { kind: `issues`; issues: IssueRow[] }
  | { kind: `runs`; runs: RunDetail[] }
  | { kind: `inbox`; notifications: NotificationRow[] }
  | { kind: `devices`; devices: DeviceRow[] }

type Screen =
  | { kind: `waiting` }
  | { kind: `error`; message: string }
  | ListScreen
  | { kind: `issue`; issue: IssueDetail; back?: ListScreen }
  | { kind: `run`; run: RunDetail; back?: ListScreen }

function list<T>(view: McpAppView, result: ToolResult): Screen {
  const decoded = decodeToolResult<T[]>(result)
  if (decoded.kind === `error`) return decoded
  const rows = Array.isArray(decoded.data) ? decoded.data : []
  if (view === `runs`) return { kind: `runs`, runs: rows as RunDetail[] }
  if (view === `inbox`) {
    return { kind: `inbox`, notifications: rows as NotificationRow[] }
  }
  if (view === `devices`) return { kind: `devices`, devices: rows as DeviceRow[] }
  return { kind: `issues`, issues: rows as IssueRow[] }
}

/** What a view's own tool result decodes to. */
export function screenFor(view: McpAppView, result: ToolResult): Screen {
  if (view === `issues` || view === `runs` || view === `inbox` || view === `devices`) {
    return list(view, result)
  }
  if (view === `issue`) {
    const decoded = decodeToolResult<IssueDetail>(result)
    return decoded.kind === `error`
      ? decoded
      : { kind: `issue`, issue: decoded.data }
  }
  const decoded = decodeToolResult<RunDetail>(result)
  return decoded.kind === `error` ? decoded : { kind: `run`, run: decoded.data }
}

export function App({ view }: { view: McpAppView }) {
  const [screen, setScreen] = useState<Screen>({ kind: `waiting` })
  const bridge = useRef<AppBridge | null>(null)
  const root = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const instance = new AppBridge({
      onToolResult: (result) => setScreen(screenFor(view, result)),
    })
    bridge.current = instance
    instance
      .connect()
      .catch(() => {
        // No host answered: the view stays on its waiting state.
      })
    return () => {
      instance.dispose()
      bridge.current = null
    }
  }, [view])

  // Exponential is dark on every client (the web forces `html.dark`), so the
  // views are too, whatever the host's own theme: they read as the app.
  useLayoutEffect(() => {
    document.documentElement.classList.add(`dark`)
  }, [])

  // The host sizes the frame to the content.
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
  const open = async (view: `issue` | `run`, id: string, back: ListScreen) => {
    const instance = bridge.current
    if (!instance) return
    try {
      const result = await instance.callTool(MCP_APP_VIEW_TOOL[view], { id })
      const next = screenFor(view, result)
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

  return (
    <McpActionsProvider value={actions}>
      <div
        ref={root}
        // The app's own card (GlassCard): the glass fill and hairline over
        // the host's ground, clipped so bands and rows follow the radius.
        className={cn(GLASS_CARD_CLASS, `overflow-hidden font-sans text-foreground antialiased`)}
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
          <IssueListView
            issues={screen.issues}
            onOpen={(row) => void open(`issue`, row.id, screen)}
          />
        )}
        {screen.kind === `runs` && (
          <RunsListView
            runs={screen.runs}
            onOpen={(run) => void open(`run`, run.id, screen)}
          />
        )}
        {screen.kind === `inbox` && (
          <InboxView
            notifications={screen.notifications}
            onOpen={(row) => row.issueId && void open(`issue`, row.issueId, screen)}
          />
        )}
        {screen.kind === `issue` && (
          <IssueDetailView
            issue={screen.issue}
            onOpenLink={openLink}
            onBack={backTo(screen.back)}
          />
        )}
        {screen.kind === `run` && (
          <RunView run={screen.run} onOpenLink={openLink} onBack={backTo(screen.back)} />
        )}
        {screen.kind === `devices` && <DevicesView devices={screen.devices} />}
      </div>
    </McpActionsProvider>
  )
}
