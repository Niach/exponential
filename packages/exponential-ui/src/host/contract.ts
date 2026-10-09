// VAPP-91: the host API contract (`catalog/host.json`) as typed constants,
// plus the message and op shapes every platform host speaks.

import hostJson from "../../catalog/host.json" with { type: "json" }
import type { FlatComponent } from "../types"

export const HOST_CONTRACT = hostJson
export const HOST_CONTRACT_VERSION: number = hostJson.version
export const A2UI_MESSAGE_KINDS: readonly string[] = hostJson.messages.a2ui
export const EXTENSION_MESSAGE_KINDS: readonly string[] = hostJson.messages.extensions
export const MESSAGE_KINDS: readonly string[] = [...hostJson.messages.a2ui, ...hostJson.messages.extensions]
export const HOST_ERROR_CODES: readonly string[] = hostJson.clientMessages.errorCodes
export const DEFAULT_URL_SCHEMES: readonly string[] = hostJson.urls.defaultSchemes
export const MCP_MIME_TYPES: readonly string[] = hostJson.transport.mcpMimeTypes
export const MCP_ACTION_TOOL: string = hostJson.transport.mcpActionTool
export const SSE_EVENTS: readonly string[] = hostJson.transport.sseEvents
/** Round 2: what a host hands each surface (`surface`): the SurfaceSettings
 *  keys, the Formatter's methods, the commands a live surface takes. */
export const SURFACE_SETTING_KEYS: readonly string[] = hostJson.surface.settings
export const FORMATTER_METHODS: readonly string[] = hostJson.surface.formatter
export const SURFACE_COMMANDS: readonly string[] = hostJson.surface.commands

export type HostErrorCode =
  | `VALIDATION_FAILED`
  | `INVALID_MESSAGE`
  | `UNSUPPORTED_CATALOG`
  | `SURFACE_NOT_FOUND`
  | `TEMPLATE_NOT_FOUND`
  | `FUNCTION_NOT_FOUND`
  | `FUNCTION_DENIED`

/** A server → client message: the four A2UI v0.9 kinds plus the two
 *  Exponential UI extensions. */
export interface ServerMessage {
  version?: string
  createSurface?: { surfaceId: string; catalogId: string; theme?: unknown; sendDataModel?: boolean }
  updateComponents?: { surfaceId: string; components: FlatComponent[] }
  updateDataModel?: { surfaceId: string; path?: string; value?: unknown }
  deleteSurface?: { surfaceId: string }
  applyTemplate?: { surfaceId: string; templateId: string; packageId?: string; data?: unknown }
  bindDataModel?: { surfaceId: string; path: string; source: string }
}

export interface ClientAction {
  name: string
  surfaceId: string
  sourceComponentId: string
  timestamp: string
  context: Record<string, unknown>
  payload?: Record<string, unknown>
}

export interface ClientError {
  code: HostErrorCode | (string & {})
  surfaceId: string
  message: string
  path?: string
}

/** A client → server message (A2UI v0.9 client_to_server). */
export type ClientMessage = { version: string; action: ClientAction } | { version: string; error: ClientError }

/** What the router turns a message into; the platform host performs the
 *  ops in order (`catalog/host.json` `ops`). */
export type HostOp =
  | { op: `create`; surfaceId: string; catalogId: string; theme?: unknown; sendDataModel?: boolean }
  | { op: `components`; surfaceId: string; components: FlatComponent[] }
  | { op: `data`; surfaceId: string; path: string; value?: unknown }
  | { op: `bind`; surfaceId: string; path: string; source: string }
  | { op: `delete`; surfaceId: string }
  | { op: `send`; message: ClientMessage }

export function errorMessage(code: HostErrorCode, surfaceId: string, message: string, path?: string): ClientMessage {
  return { version: hostJson.a2uiVersion, error: path === undefined ? { code, surfaceId, message } : { code, surfaceId, message, path } }
}

export function actionMessage(action: ClientAction): ClientMessage {
  const out: ClientAction = { name: action.name, surfaceId: action.surfaceId, sourceComponentId: action.sourceComponentId, timestamp: action.timestamp, context: action.context ?? {} }
  if (action.payload && Object.keys(action.payload).length) out.payload = action.payload
  return { version: hostJson.a2uiVersion, action: out }
}
