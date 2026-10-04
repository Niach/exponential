import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { ListEmpty, Skeleton } from "@exp/ui"
import { AppBridge, type HostContext, type ToolResult } from "./bridge"
import { IssueDetailView } from "./issue-detail-view"
import { IssueListView } from "./issue-list-view"
import { RunView } from "./run-view"
import {
  MCP_APP_VIEW_TOOL,
  decodeToolResult,
  type IssueDetail,
  type IssueRow,
  type McpAppView,
  type RunDetail,
} from "./model"

type Screen =
  | { kind: `waiting` }
  | { kind: `error`; message: string }
  | { kind: `issues`; issues: IssueRow[] }
  | { kind: `issue`; issue: IssueDetail; back?: IssueRow[] }
  | { kind: `run`; run: RunDetail }

/** What a view's own tool result decodes to. */
export function screenFor(view: McpAppView, result: ToolResult): Screen {
  if (view === `issues`) {
    const decoded = decodeToolResult<IssueRow[]>(result)
    if (decoded.kind === `error`) return decoded
    return {
      kind: `issues`,
      issues: Array.isArray(decoded.data) ? decoded.data : [],
    }
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
  const [theme, setTheme] = useState<HostContext[`theme`]>(`dark`)
  const bridge = useRef<AppBridge | null>(null)
  const root = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const instance = new AppBridge({
      onToolResult: (result) => setScreen(screenFor(view, result)),
      onHostContext: (context) => {
        if (context.theme) setTheme(context.theme)
      },
    })
    bridge.current = instance
    instance
      .connect()
      .then((context) => {
        if (context.theme) setTheme(context.theme)
      })
      .catch(() => {
        // No host answered: the view stays on its waiting state.
      })
    return () => {
      instance.dispose()
      bridge.current = null
    }
  }, [view])

  useLayoutEffect(() => {
    document.documentElement.classList.toggle(`dark`, theme !== `light`)
  }, [theme])

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

  const openIssue = async (row: IssueRow, back: IssueRow[]) => {
    const instance = bridge.current
    if (!instance) return
    try {
      const result = await instance.callTool(MCP_APP_VIEW_TOOL.issue, {
        id: row.id,
      })
      const next = screenFor(`issue`, result)
      setScreen(next.kind === `issue` ? { ...next, back } : next)
    } catch (error) {
      setScreen({
        kind: `error`,
        message: error instanceof Error ? error.message : String(error),
      })
    }
  }

  return (
    <div ref={root} className="bg-background font-sans text-foreground antialiased">
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
          onOpen={(row) => void openIssue(row, screen.issues)}
        />
      )}
      {screen.kind === `issue` && (
        <IssueDetailView
          issue={screen.issue}
          onOpenLink={openLink}
          onBack={
            screen.back
              ? () => setScreen({ kind: `issues`, issues: screen.back ?? [] })
              : undefined
          }
        />
      )}
      {screen.kind === `run` && <RunView run={screen.run} onOpenLink={openLink} />}
    </div>
  )
}
