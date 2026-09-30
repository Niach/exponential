// EXP-1153: what every view shares — the bridge lifecycle, the host theme
// mapped onto the @exp/ui tokens, the tool result as React state, and the
// size report. A view is a component given `ViewProps<Data>`.

import { StrictMode, useEffect, useState, type ReactNode } from "react"
import { createRoot } from "react-dom/client"
import { cn } from "@exp/ui/src/cn"
import { Bridge, structuredOf, type CallToolResult, type HostContext } from "./bridge"
import { VIEW_TOOLS, type ViewName } from "./contract"

export interface ViewProps<Data> {
  data: Data
  /** Re-run the view's own tool with the arguments it was rendered with. */
  refresh: () => Promise<void>
  bridge: Bridge
}

/** Host theme variables → the web theme's tokens (styles.css `:root` block).
 *  An inline style on <html> outranks both the `:root` and `.dark` blocks,
 *  so whatever the host hands over wins; what it omits keeps our value. */
const HOST_TO_TOKENS: Record<string, string[]> = {
  "--color-background-primary": [`--background`, `--popover`],
  "--color-background-secondary": [`--card`, `--muted`, `--secondary`, `--accent`, `--sidebar`],
  "--color-background-tertiary": [`--input`],
  "--color-text-primary": [`--foreground`, `--card-foreground`, `--popover-foreground`],
  "--color-text-secondary": [`--muted-foreground`, `--secondary-foreground`, `--accent-foreground`],
  "--color-border-primary": [`--border`],
  "--color-ring-primary": [`--ring`],
  "--font-sans": [`--font-sans`],
  "--font-mono": [`--font-mono`],
  "--border-radius-lg": [`--radius`],
}

function applyHostContext(ctx: HostContext | undefined, setTheme: (t: `light` | `dark`) => void) {
  if (!ctx) return
  const vars = ctx.styles?.variables ?? {}
  for (const [hostKey, tokens] of Object.entries(HOST_TO_TOKENS)) {
    const value = vars[hostKey]
    if (!value) continue
    for (const token of tokens) document.documentElement.style.setProperty(token, value)
  }
  if (ctx.styles?.css?.fonts) {
    const style = document.createElement(`style`)
    style.textContent = ctx.styles.css.fonts
    document.head.appendChild(style)
  }
  if (ctx.theme) setTheme(ctx.theme)
}

type Phase<Data> =
  | { kind: `waiting` }
  | { kind: `ready`; data: Data }
  | { kind: `error`; message: string }
  | { kind: `cancelled`; reason: string }

export function mountView<Data>(view: ViewName, View: (props: ViewProps<Data>) => ReactNode) {
  const bridge = new Bridge()
  let toolName = VIEW_TOOLS[view]
  let toolArgs: Record<string, unknown> = {}

  function Shell() {
    const [phase, setPhase] = useState<Phase<Data>>({ kind: `waiting` })
    const [theme, setTheme] = useState<`light` | `dark`>(`dark`)

    useEffect(() => {
      const apply = (result: CallToolResult) => {
        if (result.isError) {
          setPhase({ kind: `error`, message: result.content?.[0]?.text ?? `The tool failed.` })
          return
        }
        const data = structuredOf<Data>(result)
        if (!data) setPhase({ kind: `error`, message: `Unexpected result shape.` })
        else setPhase({ kind: `ready`, data })
      }
      bridge.on(`ui/notifications/tool-input`, (p) => {
        toolArgs = (p.arguments as Record<string, unknown>) ?? {}
      })
      bridge.on(`ui/notifications/tool-result`, (p) => apply(p as CallToolResult))
      bridge.on(`ui/notifications/tool-cancelled`, (p) =>
        setPhase({ kind: `cancelled`, reason: String(p.reason ?? ``) })
      )
      bridge.on(`ui/notifications/host-context-changed`, (p) =>
        applyHostContext(p as HostContext, setTheme)
      )
      bridge.on(`ui/resource-teardown`, () => ({}))
      void (async () => {
        const init = await bridge.request<{ hostContext?: HostContext }>(`ui/initialize`, {
          protocolVersion: `2025-11-25`,
          appInfo: { name: `exponential-${view}`, version: `0.1.0` },
          appCapabilities: { availableDisplayModes: [`inline`, `fullscreen`] },
        })
        const hostTool = init?.hostContext?.toolInfo?.tool?.name
        if (hostTool) toolName = hostTool
        applyHostContext(init?.hostContext, setTheme)
        bridge.notify(`ui/notifications/initialized`)
      })()
      const observer = new ResizeObserver(() => bridge.reportSize())
      observer.observe(document.body)
      return () => observer.disconnect()
    }, [])

    const refresh = async () => {
      try {
        const result = await bridge.callTool<Data>(toolName, toolArgs)
        if (result.isError) {
          setPhase({ kind: `error`, message: result.content?.[0]?.text ?? `Refresh failed.` })
          return
        }
        const data = structuredOf<Data>(result)
        if (data) setPhase({ kind: `ready`, data })
      } catch (e) {
        setPhase({ kind: `error`, message: e instanceof Error ? e.message : String(e) })
      }
    }

    return (
      <div className={cn(theme === `dark` && `dark`, `font-sans text-foreground antialiased`)}>
        <div className="rounded-lg border border-border bg-background">
          {phase.kind === `waiting` && <Empty>Loading…</Empty>}
          {phase.kind === `cancelled` && <Empty>Cancelled{phase.reason ? `: ${phase.reason}` : ``}.</Empty>}
          {phase.kind === `error` && (
            <div className="p-3 text-sm text-destructive">{phase.message}</div>
          )}
          {phase.kind === `ready` && <View data={phase.data} refresh={refresh} bridge={bridge} />}
        </div>
      </div>
    )
  }

  createRoot(document.getElementById(`root`)!).render(
    <StrictMode>
      <Shell />
    </StrictMode>
  )
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="px-3 py-7 text-center text-sm text-muted-foreground">{children}</div>
}
