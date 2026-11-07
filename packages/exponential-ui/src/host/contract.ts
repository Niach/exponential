// VAPP-91: the host API contract (`catalog/host.json`) as typed constants,
// plus the message and op shapes every platform host speaks.

// The runtime slice (generated/host.runtime.json, `bun run generate`): a
// renderer bundle never ships the package-format spec or the prose.
import hostJson from "../../generated/host.runtime.json" with { type: "json" }
import type { FlatComponent } from "../types"

export const HOST_CONTRACT_VERSION: number = hostJson.version
export const A2UI_MESSAGE_KINDS: readonly string[] = hostJson.messages.a2ui
/** Round 9 §2: the host-internal kinds (only through a TRUSTED route). */
export const HOST_MESSAGE_KINDS: readonly string[] = hostJson.messages.host
/** The pre-1.0 name of `HOST_MESSAGE_KINDS`. */
export const EXTENSION_MESSAGE_KINDS: readonly string[] = HOST_MESSAGE_KINDS
export const MESSAGE_KINDS: readonly string[] = [...hostJson.messages.a2ui, ...hostJson.messages.host]
/** Round 9 §1.3: the renderer → agent kinds. */
export const CLIENT_MESSAGE_KINDS: readonly string[] = hostJson.clientMessages.kinds
export const HOST_ERROR_CODES: readonly string[] = hostJson.clientMessages.errorCodes
export const DEFAULT_URL_SCHEMES: readonly string[] = hostJson.urls.defaultSchemes
/** The media loader's schemes when the host lists none (no `file`). */
export const DEFAULT_MEDIA_SCHEMES: readonly string[] = hostJson.media.defaultSchemes
/** What every image loader enforces (`media.limits`). */
export const MEDIA_LIMITS: { readonly maxBytes: number; readonly timeoutMs: number; readonly maxPixels: number } = hostJson.media.limits
/** The renderer → host hook a failed painter calls (`paint.hook`) and the
 *  A2UI error code the host forwards it with (`paint.errorCode`). */
export const PAINT_ERROR_HOOK: string = hostJson.paint.hook
export const RENDER_FAILED = `RENDER_FAILED` as const
export const MCP_MIME_TYPES: readonly string[] = hostJson.transport.mcpMimeTypes
export const MCP_ACTION_TOOL: string = hostJson.transport.mcpActionTool
export const SSE_EVENTS: readonly string[] = hostJson.transport.sseEvents
/** Round 2: what a host hands each surface (`surface`): the SurfaceSettings
 *  keys, the Formatter's methods, the commands a live surface takes. */
export const SURFACE_SETTING_KEYS: readonly string[] = hostJson.surface.settings
export const FORMATTER_METHODS: readonly string[] = hostJson.surface.formatter
export const SURFACE_COMMANDS: readonly string[] = hostJson.surface.commands
/** #VAPP-258: the optional hooks a Markdown asks (`markdown`). */
export const MARKDOWN_HOST_HOOKS: readonly string[] = hostJson.markdown.hooks

export type HostErrorCode =
  | `VALIDATION_FAILED`
  | `UNALLOWED_PARENT`
  | `UNALLOWED_CHILD`
  | `INTEGRITY_ERROR`
  | `INVALID_FUNCTION_CALL`
  | `EXPRESSION_ERROR`
  | `EXECUTION_ERROR`
  | `UNKNOWN_FUNCTION`
  | `INVALID_MESSAGE`
  | `UNSUPPORTED_CATALOG`
  | `SURFACE_NOT_FOUND`
  | `TEMPLATE_NOT_FOUND`
  | `SOURCE_DENIED`
  | `RENDER_FAILED`

/** The codes of the VALIDATION branch: `{code, surfaceId, message, path}`,
 *  path always set (round 9 §1.3). */
const VALIDATION_CODES: readonly string[] = [`VALIDATION_FAILED`, `UNALLOWED_PARENT`, `UNALLOWED_CHILD`]

/** An agent → renderer message: the six A2UI 1.0 kinds plus the two
 *  host-internal ones (`docs/round-9-contract.md` §1.2, §2). */
export interface ServerMessage {
  version?: string
  createSurface?: { surfaceId: string; catalogId?: string; components?: FlatComponent[]; dataModel?: Record<string, unknown>; sendDataModel?: boolean; metadata?: { extensions?: Record<string, unknown> } }
  updateComponents?: { surfaceId: string; components: FlatComponent[] }
  updateDataModel?: { surfaceId: string; path?: string; value: unknown }
  deleteSurface?: { surfaceId: string }
  callRendererFunction?: { surfaceId?: string; functionCallId: string; callFunction: { "@call": string; catalogId: string; args?: Record<string, unknown> } }
  agentFunctionResponse?: { functionCallId: string; value?: unknown; error?: { code: string; message: string } }
  applyTemplate?: { surfaceId: string; templateId: string; packageId?: string; data?: unknown }
  bindDataModel?: { surfaceId: string; path: string; source: string }
}

export interface ClientAction {
  name: string
  surfaceId: string
  /** The AUTHORED id: the painted id up to its first `.` (round 9 §4.3). */
  sourceComponentId: string
  timestamp: string
  context: Record<string, unknown>
  userMessage?: string
  metadata?: { extensions: Record<string, unknown> }
}

/** Round 9 §1.3: exactly one of `surfaceId` / `functionCallId`. */
export interface ClientError {
  code: HostErrorCode | (string & {})
  surfaceId?: string
  functionCallId?: string
  message: string
  path?: string
}

/** A renderer → agent message (A2UI 1.0, round 9 §1.3). */
export type ClientMessage =
  | { version: string; action: ClientAction }
  | { version: string; error: ClientError }
  | { version: string; callAgentFunction: { surfaceId: string; functionCallId: string; callFunction: { "@call": string; catalogId: string; args: Record<string, unknown> } } }
  | { version: string; rendererFunctionResponse: { functionCallId: string; value?: unknown; error?: { code: string; message: string } } }

/** What the router turns a message into; the platform host performs the
 *  ops in order (`catalog/host.json` `ops`). */
export type HostOp =
  | { op: `create`; surfaceId: string; catalogId?: string; sendDataModel?: boolean; metadata?: { extensions?: Record<string, unknown> }; theme?: unknown }
  | { op: `components`; surfaceId: string; components: FlatComponent[] }
  | { op: `data`; surfaceId: string; path: string; value?: unknown }
  | { op: `bind`; surfaceId: string; path: string; source: string }
  | { op: `delete`; surfaceId: string }
  | { op: `send`; message: ClientMessage }
  | { op: `call`; functionCallId: string; surfaceId?: string; catalogId: string; name: string; args: Record<string, unknown> }
  | { op: `respond`; functionCallId: string; value: unknown }
  | { op: `respond`; functionCallId: string; error: { code: string; message: string } }

/** Who an `error` is about (round 9 §1.3: exactly one of the two). */
export type ErrorTarget = { surfaceId: string } | { functionCallId: string }

/** A renderer `error` (round 9 §1.3): keys `code`, `surfaceId` |
 *  `functionCallId`, `message`, `path`. A string target is a surface id. The
 *  validation codes always carry a `path` (`/` when none is given). */
export function errorMessage(code: HostErrorCode, target: ErrorTarget | string, message: string, path?: string): ClientMessage {
  const to = typeof target === `string` ? { surfaceId: target } : target
  const at = path ?? (VALIDATION_CODES.includes(code) && `surfaceId` in to ? `/` : undefined)
  return { version: hostJson.a2uiVersion, error: { code, ...to, message, ...(at === undefined ? {} : { path: at }) } }
}

/** The authored id of a painted id: up to its first `.` (round 9 §4.3). */
export const authoredId = (painted: string): string => {
  const at = painted.indexOf(`.`)
  return at < 0 ? painted : painted.slice(0, at)
}

/** What `actionMessage` takes: the painted id in `sourceComponentId`, the
 *  component's own event data in `payload`. */
export interface ActionInput {
  name: string
  surfaceId: string
  sourceComponentId: string
  timestamp: string
  context?: Record<string, unknown> | null
  payload?: unknown
  userMessage?: string
}

/** Round 9 §1.3: keys `name`, `surfaceId`, `sourceComponentId` (the AUTHORED
 *  id), `timestamp`, `context`, `userMessage`, then `metadata` with
 *  `at_exponential_payload` (the component's own event data, when it has
 *  keys) and `at_exponential_instance` (the painted id, when it differs). */
export function actionMessage(action: ActionInput): ClientMessage {
  const authored = authoredId(action.sourceComponentId)
  const out: ClientAction = { name: action.name, surfaceId: action.surfaceId, sourceComponentId: authored, timestamp: action.timestamp, context: action.context ?? {} }
  if (action.userMessage !== undefined) out.userMessage = action.userMessage
  const extensions: Record<string, unknown> = {}
  const payload = action.payload
  if (typeof payload === `string` ? payload !== `` : Array.isArray(payload) ? payload.length > 0 : typeof payload === `object` && payload !== null && Object.keys(payload).length > 0) extensions.at_exponential_payload = payload
  if (authored !== action.sourceComponentId) extensions.at_exponential_instance = action.sourceComponentId
  if (Object.keys(extensions).length > 0) out.metadata = { extensions }
  return { version: hostJson.a2uiVersion, action: out }
}

/** Round 9 §1.3 `callAgentFunction`. */
export function callAgentFunctionMessage(surfaceId: string, functionCallId: string, name: string, catalogId: string, args: Record<string, unknown>): ClientMessage {
  return { version: hostJson.a2uiVersion, callAgentFunction: { surfaceId, functionCallId, callFunction: { "@call": name, catalogId, args } } }
}

/** Round 9 §1.3 `rendererFunctionResponse`: a function returning nothing
 *  answers `value: null`. */
export function rendererFunctionResponse(functionCallId: string, result: { value?: unknown } | { error: { code: string; message: string } }): ClientMessage {
  return { version: hostJson.a2uiVersion, rendererFunctionResponse: `error` in result ? { functionCallId, error: result.error } : { functionCallId, value: result.value ?? null } }
}
