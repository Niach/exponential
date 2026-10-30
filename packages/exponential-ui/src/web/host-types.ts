// VAPP-87: what an EMBEDDING APP provides (the host plugin) and what a web
// renderer sends it, framework-free. The plugin knows no transport: actions
// and input edits are plain objects the host forwards wherever it likes.
// VAPP-91: the host API (`ExponentialHost`: transport, functions, bindings,
// negotiation, policy) plugs in through a renderer's `hostPlugin(host)`.
// FEED-128: generic over the renderer's component types (`Icon` = what an
// icon registry name maps to, `Markdown` = a richer markdown renderer), so
// React and Solid share ONE plugin shape.

import type { FunctionCallInfo, MediaOptions, MediaRequest, ThemeIssue, UiNode, UrlPolicy } from "@exponential_app/ui"
import type { ClientFunction } from "./data"
import type { QrScannerHost } from "./round5"

/** A registry name → the renderer's icon component (a map or a lookup). */
export type IconMap<Icon = unknown> = Record<string, Icon | undefined> | ((name: string) => Icon | undefined)

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

/** A painter that failed: the surface, the component, why. */
export interface PaintError {
  surfaceId: string
  componentId: string
  message: string
}

/** One option a host source supplies (the catalog's `option` shape). */
export interface SourceOption {
  label: string
  value: string
  icon?: string
  disabled?: boolean
}

export interface HostPlugin<Icon = unknown, Markdown = unknown> {
  /** The icon registry: the catalog's `Icon` names → components. */
  icons?: IconMap<Icon>
  /** Server events. Returning a promise keeps the source control in its
   *  optimistic pending state until it settles. */
  onAction?: (event: SurfaceActionEvent) => void | Promise<void>
  /** Host-owned input edits. Returning a promise acknowledges the revision
   *  when it settles; a sync host acknowledges at once. */
  onInput?: (event: SurfaceInputEvent) => void | Promise<void>
  /** Opens an href the URL policy allowed (`openUrl`, `Link`); default
   *  `window.open(url, "_blank")`. */
  openUrl?: (url: string) => void
  /** The URL policy EVERY href passes (Link, markdown links, FileUpload
   *  file urls, openUrl): `catalog/host.json` urls. Unset = the default
   *  schemes; relative urls resolve against `baseUrl`, else `media.baseUrl`,
   *  else the document's. A denied href paints as text. */
  urls?: UrlPolicy
  /** The media policy every src passes when there is no `mediaRequest`
   *  (schemes, hosts, private opt-ins, base, header rules):
   *  `catalog/host.json` media. */
  media?: MediaOptions
  /** Extra or overriding client functions, web renderers only (`{call,
   *  args}` in a prop; an action naming one runs it). Any OTHER action
   *  function name goes to onFunctionCall (the portable host functions). */
  functions?: Record<string, ClientFunction>
  /** An action `functionCall` to a name outside the built-ins (the host
   *  function registry + its policy gate, `ExponentialHost.callFunction`).
   *  A promise keeps the source control pending until it settles. */
  onFunctionCall?: (call: FunctionCallInfo) => unknown | Promise<unknown>
  /** The media loader's request for a source (absolute url + headers, e.g.
   *  auth for /api/attachments); null = denied. A request WITH headers is
   *  fetched (under the media limits) and shown as a blob url; without, its
   *  url is used as is. Its url is re-checked against `media`'s schemes,
   *  hosts and private opt-ins. */
  mediaRequest?: (src: string) => MediaRequest | null
  /** Video/Audio (`catalog/host.json` media: the byte limits do not apply,
   *  they stream): a url the browser's player loads WITHOUT headers for a
   *  request that carries some (a signed or cookie-authed url), so the
   *  player streams and seeks. Its result passes the media policy again.
   *  Unset = the request is fetched whole into a blob url under the media
   *  byte cap and timeout for the WHOLE download (VAPP-107), so a long
   *  video needs this hook. */
  mediaStreamUrl?: (request: MediaRequest) => string | Promise<string>
  /** A richer markdown renderer than the built-in one (a component taking
   *  `{text, className}` in the renderer's framework). */
  Markdown?: Markdown
  /** Rewrites a media src BEFORE the media policy (signed URLs). */
  resolveUrl?: (src: string) => string
  /** `onPaintError` (`catalog/host.json` paint): a component's painter
   *  failed; it paints an empty box. `hostPlugin(host)` forwards it to the
   *  agent as an A2UI `RENDER_FAILED` error. */
  onPaintError?: (error: PaintError) => void
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
  /** VAPP-162, QrScanner: THE camera (host-owned, like FileUpload's bytes):
   *  `start(feed, formats, onValue)` renders the live feed into `feed`,
   *  decodes codes of `formats` and calls `onValue` per decoded string;
   *  returns the stop. Unset (or `start` returning false) = no camera: the
   *  scanner shows its placeholder and `Camera unavailable`. */
  qrScanner?: QrScannerHost
}

/** Any renderer's plugin (what the framework-free helpers take). */
export type AnyHostPlugin = HostPlugin<unknown, unknown>

/** A registry lookup: the host's map (or function) first, then the
 *  renderer's own fallbacks. */
export function lookupIcon<Icon>(icons: IconMap<Icon> | undefined, name: string, fallbacks: Readonly<Record<string, Icon>> = {}): Icon | undefined {
  if (!icons) return fallbacks[name]
  const hit = typeof icons === `function` ? icons(name) : icons[name]
  return hit ?? fallbacks[name]
}
