/* The ONE way the site paints a surface: A2UI messages → `useSurface` (the
   TS reference reducer as React state) → `<ExponentialSurface>`. A stub host
   echoes inputs into the data model after a short round trip and reports
   actions to the page. Client-only (loaded by the islands, never by the
   prerender). */
import { useEffect, useLayoutEffect, useMemo, useRef } from "react"
import { ExponentialSurface, useSurface } from "@exponential-at/ui-react"
import type { A2uiMessage, HostPlugin, IconMap, SurfaceActionEvent, ThemeInput } from "@exponential-at/ui-react"
import type { ExtensionDef, ModeName, ReduceIssue } from "@exponential-at/ui"
import type { Message } from "./a2ui"

export interface LiveSurfaceProps {
  surfaceId: string
  /** Unique per page when several surfaces share a surface id. */
  domId?: string
  messages: readonly Message[]
  theme: ThemeInput
  mode: ModeName
  icons?: IconMap
  width?: number | string
  extensions?: readonly ExtensionDef[]
  onIssues?: (issues: ReduceIssue[]) => void
  onAction?: (event: SurfaceActionEvent) => void
}

const NO_EXTENSIONS: readonly ExtensionDef[] = []
const useIsoLayoutEffect = typeof window === `undefined` ? useEffect : useLayoutEffect

export function LiveSurface({ surfaceId, domId, messages, theme, mode, icons, width, extensions = NO_EXTENSIONS, onIssues, onAction }: LiveSurfaceProps) {
  const surface = useSurface({ surfaceId, extensions })
  const { apply, reset, setData } = surface

  // Replay the whole message list whenever it changes (a stream grows it).
  useIsoLayoutEffect(() => {
    reset()
    for (const m of messages) apply(m as A2uiMessage)
  }, [messages, apply, reset])

  const issuesRef = useRef(onIssues)
  issuesRef.current = onIssues
  useEffect(() => {
    issuesRef.current?.(surface.issues)
  }, [surface.issues])

  const actionRef = useRef(onAction)
  actionRef.current = onAction
  const host = useMemo<HostPlugin>(
    () => ({
      icons,
      onAction: (e) => {
        actionRef.current?.(e)
        return new Promise((r) => setTimeout(r, 150))
      },
      onInput: (e) =>
        new Promise<void>((resolve) =>
          setTimeout(() => {
            if (e.path) setData(e.path, e.value)
            resolve()
          }, 120)
        ),
      openUrl: (url) => window.open(url, `_blank`, `noopener,noreferrer`),
    }),
    [icons, setData]
  )

  return <ExponentialSurface id={domId ?? `xui-${surfaceId.replace(/[^a-zA-Z0-9_-]/g, ``)}`} surface={surface} host={host} theme={theme} mode={mode} width={width} />
}
