// VAPP-87: what an EMBEDDING APP provides (the host plugin) and what the
// renderer sends it. The plugin knows no transport: actions and input edits
// are plain objects the host forwards wherever it likes. VAPP-91: the host
// API (`@exponential-at/ui` `ExponentialHost`: transport, functions,
// bindings, negotiation, policy) plugs in through `hostPlugin(host)` /
// `<HostSurface>` (host-surface.tsx).

import type { ComponentType, SVGProps } from "react"
import type { UiNode, ResolvedTheme, ModeName, FunctionCallInfo, MediaRequest, ThemeIssue } from "@exponential-at/ui"
import type { ClientFunction } from "./data"

/** An icon component the host maps a registry name to (`lucide-react`'s
 *  `LucideIcon` shape fits). */
export type IconComponent = ComponentType<SVGProps<SVGSVGElement> & { size?: number | string; className?: string }>
export type IconMap = Record<string, IconComponent | undefined> | ((name: string) => IconComponent | undefined)

/** A user action: a node's `on.<event>` with an `event` action, resolved. */
export interface SurfaceActionEvent {
  surfaceId: string
  /** The catalog event (`press`, `change`, `select`, `submit`…). */
  event: string
  /** The action's `event.name`. */
  name: string
  componentId: string
  context: Record<string, unknown>
  /** What the component adds (a Select's `value`, a Tabs' `value`…). */
  payload?: Record<string, unknown>
  timestamp: string
}

/** A host-owned input edit. `change` fires debounced while typing with a
 *  monotonically increasing `revision`; `commit` on blur / Enter. The host's
 *  echo (a data model write at `path`) is applied only while no newer
 *  revision is outstanding and the field is not focused. */
export interface SurfaceInputEvent {
  surfaceId: string
  componentId: string
  /** The input's `name` prop. */
  name: string
  /** The bound data model pointer, when `value` is a binding. */
  path?: string
  value: unknown
  revision: number
  kind: `change` | `commit`
}

export interface HostPlugin {
  /** The icon registry: the catalog's `Icon` names → components. */
  icons?: IconMap
  /** Server events. Returning a promise keeps the source control in its
   *  optimistic pending state until it settles. */
  onAction?: (event: SurfaceActionEvent) => void | Promise<void>
  /** Host-owned input edits. Returning a promise acknowledges the revision
   *  when it settles; a sync host acknowledges at once. */
  onInput?: (event: SurfaceInputEvent) => void | Promise<void>
  /** `openUrl` and `Link`; default `window.open(url, "_blank")`. */
  openUrl?: (url: string) => void
  /** Extra or overriding client functions, React only (`{call, args}` in a
   *  prop; an action naming one runs it). Any OTHER action function name
   *  goes to onFunctionCall (the portable host functions). */
  functions?: Record<string, ClientFunction>
  /** An action `functionCall` to a name outside the built-ins (the host
   *  function registry + its policy gate, `ExponentialHost.callFunction`).
   *  A promise keeps the source control pending until it settles. */
  onFunctionCall?: (call: FunctionCallInfo) => unknown | Promise<unknown>
  /** The media loader's request for a source (absolute url + headers, e.g.
   *  auth for /api/attachments). A request WITH headers is fetched and shown
   *  as a blob url; without, its url is used as is. */
  mediaRequest?: (src: string) => MediaRequest | null
  /** A richer markdown renderer than the built-in one. */
  Markdown?: ComponentType<{ text: string; className?: string }>
  /** Rewrites media URLs (relative attachment paths, signed URLs). */
  resolveUrl?: (src: string) => string
  /** Called with the node for every Unknown placeholder painted. */
  onUnknown?: (node: UiNode) => void
  /** Round 4 (VAPP-103): the `theme` prop was unusable (an unknown built-in
   *  id, a theme file with issues). The surface never throws: it paints
   *  with the default theme and reports why here. */
  onThemeIssues?: (issues: ThemeIssue[]) => void
  /** Round 1, FileUpload: the picked/dropped files' BYTES (the `upload`
   *  event carries only `{name, size, type}`). Return a promise to keep the
   *  drop zone busy until it settles. */
  onUpload?: (files: File[], target: { nodeId: string; name: string }) => void | Promise<void>
  /** Round 1, Select `source`: a host list id (`exp:statuses`) → its
   *  options for a query (`""` when not searchable). */
  optionSource?: (source: string, query: string) => SourceOption[] | Promise<SourceOption[]>
}

/** One option a host source supplies (the catalog's `option` shape). */
export interface SourceOption {
  label: string
  value: string
  icon?: string
  disabled?: boolean
}

/** What an extension component receives: the node, its props resolved, the
 *  theme and mode, the rendered children, and an emitter for its events. */
export interface ExtensionComponentProps {
  node: UiNode
  props: Record<string, unknown>
  theme: ResolvedTheme
  mode: ModeName
  tokens: ResolvedTheme[`tokens`]
  /** The node's children, already rendered. */
  children?: React.ReactNode
  /** Rendered slots by name. */
  slots: Record<string, React.ReactNode>
  /** Fire one of the node's `on` handlers. */
  emit: (event: string, payload?: Record<string, unknown>) => void
  /** The element attributes the renderer wants on the root (classes, data). */
  rootProps: Record<string, unknown>
}
