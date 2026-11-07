// VAPP-265: capabilities v1. The tools' schemas, the JSON-schema subset that
// checks them, and THE BRIDGE between the broker's `call` verdict and a host's
// native provider (the same for app UI and agents). Pure and browser-safe;
// locked by `fixtures/capabilities.json` (`tools`, `validateCases`, `bridgeCases`).

import type { HubOutcome } from "@exp/emcp"

import fixture from "../fixtures/capabilities.json" with { type: "json" }
import { renderPdf } from "./pdf"

export type JsonSchema = Record<string, unknown>

export interface CapabilityTool {
  name: string
  runs: `native` | `engine`
  description: string
  inputSchema: JsonSchema
  nativeOutputSchema: JsonSchema | null
  outputSchema: JsonSchema
}

export const CAPABILITY_TOOLS = fixture.tools as unknown as readonly CapabilityTool[]
export const MIME_BY_EXTENSION: Readonly<Record<string, string>> = fixture.v1.mimeByExtension

export const toolDef = (name: string): CapabilityTool | null => CAPABILITY_TOOLS.find((t) => t.name === name) ?? null

/** A host's native provider of one capability (`name`, like the crate's
 *  `Capability`); `call` gets the bare tool (`pick`) and the mapped native
 *  arguments, and answers an MCP call result (`structuredContent` per
 *  `nativeOutputSchema`, or `isError`). */
export interface NativeCapability {
  name: string
  call(tool: string, args: Record<string, unknown>): Promise<unknown>
}

/** `bridgeCall`'s `native` over a host's capabilities: `photos_pick` → the
 *  `photos` provider's `pick`; a capability the host lacks answers `unavailable`. */
export const nativeCaller =
  (capabilities: readonly NativeCapability[]) =>
  async (tool: string, args: Record<string, unknown>): Promise<unknown> => {
    const at = tool.indexOf(`_`)
    const capability = capabilities.find((c) => c.name === tool.slice(0, at))
    if (!capability) return { content: [{ type: `text`, text: `unavailable` }], isError: true }
    return capability.call(tool.slice(at + 1), args)
  }

// ---------------------------------------------------------------------------
// The schema subset

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

function typeMatches(type: string, v: unknown): boolean {
  switch (type) {
    case `object`:
      return isObject(v)
    case `array`:
      return Array.isArray(v)
    case `string`:
      return typeof v === `string`
    case `boolean`:
      return typeof v === `boolean`
    case `null`:
      return v === null
    case `number`:
      return typeof v === `number` && Number.isFinite(v)
    case `integer`:
      return typeof v === `number` && Number.isInteger(v)
    default:
      return false
  }
}

function jsonEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true
  if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((x, i) => jsonEqual(x, b[i]))
  if (isObject(a) && isObject(b)) {
    const ka = Object.keys(a)
    return ka.length === Object.keys(b).length && ka.every((k) => k in b && jsonEqual(a[k], b[k]))
  }
  return false
}

const pointer = (path: readonly string[]): string => (path.length ? path.map((p) => `/${p.replace(/~/g, `~0`).replace(/\//g, `~1`)}`).join(``) : `/`)

const DATE_TIME = /^(\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})(\.\d+)?([Zz]|[+-](\d{2}):(\d{2}))$/
const TOKEN = /^[A-Za-z0-9._-]{1,64}$/
const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/

function dateTimeValid(s: string): boolean {
  const m = DATE_TIME.exec(s)
  if (!m) return false
  const [mo, d, h, mi, se] = [m[2], m[3], m[4], m[5], m[6]].map(Number)
  if (mo < 1 || mo > 12 || d < 1 || d > 31 || h > 23 || mi > 59 || se > 60) return false
  if (m[9] !== undefined && (Number(m[9]) > 23 || Number(m[10]) > 59)) return false
  return true
}

function uriValid(s: string): boolean {
  if (!/^https?:\/\/[^\s]+$/i.test(s) || /[\u0000-\u001f\u007f]/.test(s)) return false
  try {
    const url = new URL(s)
    return (url.protocol === `http:` || url.protocol === `https:`) && url.hostname !== ``
  } catch {
    return false
  }
}

function formatValid(format: string, s: string): boolean {
  switch (format) {
    case `date-time`:
      return dateTimeValid(s)
    case `token`:
      return TOKEN.test(s)
    case `uri`:
      return uriValid(s)
    case `base64`:
      return base64Decode(s) !== null
    default:
      return true
  }
}

/** The first violation of `value` against `schema` as `<pointer> <keyword>`, or null. */
export function schemaViolation(schema: JsonSchema, value: unknown, path: readonly string[] = []): string | null {
  const at = (keyword: string, p: readonly string[] = path) => `${pointer(p)} ${keyword}`
  if (schema.type !== undefined) {
    const types = Array.isArray(schema.type) ? (schema.type as string[]) : [schema.type as string]
    if (!types.some((t) => typeMatches(t, value))) return at(`type`)
  }
  if (isObject(value)) {
    const properties = (schema.properties ?? {}) as Record<string, JsonSchema>
    for (const key of (schema.required ?? []) as string[]) if (!(key in value)) return at(`required`, [...path, key])
    if (schema.additionalProperties === false) {
      const unknown = Object.keys(value).filter((k) => !(k in properties)).sort()
      if (unknown.length) return at(`additionalProperties`, [...path, unknown[0]])
    }
    for (const [key, sub] of Object.entries(properties)) {
      if (!(key in value)) continue
      const v = schemaViolation(sub, value[key], [...path, key])
      if (v) return v
    }
    for (const [key, needs] of Object.entries((schema.dependentRequired ?? {}) as Record<string, string[]>)) {
      if (!(key in value)) continue
      for (const need of needs) if (!(need in value)) return at(`dependentRequired`, [...path, need])
    }
  }
  if (schema.anyOf && !(schema.anyOf as JsonSchema[]).some((s) => schemaViolation(s, value, path) === null)) return at(`anyOf`)
  if (schema.oneOf && (schema.oneOf as JsonSchema[]).filter((s) => schemaViolation(s, value, path) === null).length !== 1) return at(`oneOf`)
  if (Array.isArray(value)) {
    if (typeof schema.minItems === `number` && value.length < schema.minItems) return at(`minItems`)
    if (typeof schema.maxItems === `number` && value.length > schema.maxItems) return at(`maxItems`)
    if (schema.items) {
      for (let i = 0; i < value.length; i++) {
        const v = schemaViolation(schema.items as JsonSchema, value[i], [...path, String(i)])
        if (v) return v
      }
    }
  }
  if (typeof value === `number`) {
    if (typeof schema.minimum === `number` && value < schema.minimum) return at(`minimum`)
    if (typeof schema.maximum === `number` && value > schema.maximum) return at(`maximum`)
  }
  if (typeof value === `string`) {
    const length = [...value].length
    if (typeof schema.minLength === `number` && length < schema.minLength) return at(`minLength`)
    if (typeof schema.maxLength === `number` && length > schema.maxLength) return at(`maxLength`)
  }
  if (schema.enum && !(schema.enum as unknown[]).some((e) => jsonEqual(e, value))) return at(`enum`)
  if (typeof value === `string` && typeof schema.format === `string` && !formatValid(schema.format, value)) return at(`format`)
  return null
}

export type ValidateResult = { arguments: Record<string, unknown> } | { error: string }

/** A call's arguments against the tool's `inputSchema`; a missing top-level
 *  property with a `default` gets it. */
export function validateToolArguments(tool: string, args: unknown): ValidateResult {
  const def = toolDef(tool)
  if (!def) return { error: `unknownTool` }
  const violation = schemaViolation(def.inputSchema, args)
  if (violation) return { error: `invalidParams: ${violation}` }
  const out: Record<string, unknown> = { ...(args as Record<string, unknown>) }
  for (const [key, sub] of Object.entries((def.inputSchema.properties ?? {}) as Record<string, JsonSchema>)) {
    if (!(key in out) && `default` in sub) out[key] = structuredClone(sub.default)
  }
  return { arguments: out }
}

// ---------------------------------------------------------------------------
// Bytes

const B64 = `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/`

export function base64Encode(bytes: Uint8Array): string {
  let out = ``
  for (let i = 0; i < bytes.length; i += 3) {
    const n = (bytes[i] << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0)
    out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63]
    out += i + 1 < bytes.length ? B64[(n >> 6) & 63] : `=`
    out += i + 2 < bytes.length ? B64[n & 63] : `=`
  }
  return out
}

/** Strict: the standard alphabet, padded, no whitespace, zero trailing bits; null otherwise. */
export function base64Decode(s: string): Uint8Array | null {
  if (!BASE64.test(s)) return null
  const pad = s.endsWith(`==`) ? 2 : s.endsWith(`=`) ? 1 : 0
  const out = new Uint8Array((s.length / 4) * 3 - pad)
  let o = 0
  for (let i = 0; i < s.length; i += 4) {
    const c = [0, 1, 2, 3].map((k) => (s[i + k] === `=` ? 0 : B64.indexOf(s[i + k])))
    const n = (c[0] << 18) | (c[1] << 12) | (c[2] << 6) | c[3]
    const last = i + 4 === s.length
    if (last && pad === 2 && (n & 0xffff) !== 0) return null
    if (last && pad === 1 && (n & 0xff) !== 0) return null
    out[o++] = (n >> 16) & 255
    if (o < out.length) out[o++] = (n >> 8) & 255
    if (o < out.length) out[o++] = n & 255
  }
  return out
}

const startsWith = (bytes: Uint8Array, prefix: readonly number[]) => prefix.every((b, i) => bytes[i] === b)
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] as const

/** The type the bytes tell (PDF, JPEG, PNG, GIF), or null. */
export function sniffMime(bytes: Uint8Array): string | null {
  if (startsWith(bytes, [0x25, 0x50, 0x44, 0x46, 0x2d])) return `application/pdf`
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) return `image/jpeg`
  if (startsWith(bytes, PNG_SIGNATURE)) return `image/png`
  if (startsWith(bytes, [0x47, 0x49, 0x46, 0x38])) return `image/gif`
  return null
}

/** A file's MIME type: the given one, else its name's extension, else its bytes,
 *  else text/plain for a text and application/octet-stream. */
export function mimeFor(name: string, bytes: Uint8Array, given: string | null | undefined, isText: boolean): string {
  if (given) return given
  const dot = name.lastIndexOf(`.`)
  const ext = dot >= 0 ? name.slice(dot + 1).toLowerCase() : ``
  if (ext && Object.hasOwn(MIME_BY_EXTENSION, ext)) return MIME_BY_EXTENSION[ext]
  return sniffMime(bytes) ?? (isText ? `text/plain` : `application/octet-stream`)
}

/** A notification id the app did not name: `n-` + 32 lowercase hex. */
export function newNotificationId(): string {
  const bytes = new Uint8Array(16)
  globalThis.crypto.getRandomValues(bytes)
  return `n-${[...bytes].map((b) => b.toString(16).padStart(2, `0`)).join(``)}`
}

// ---------------------------------------------------------------------------
// The bridge

export interface BlobStore {
  get(app: string, id: string): Promise<Uint8Array | null>
  /** Stores the bytes as an app file; answers its id. */
  put(app: string, name: string, mimeType: string, bytes: Uint8Array): Promise<string>
}

export interface BridgeContext {
  app: string
  screens: readonly string[]
}

export interface McpCallResult {
  content: { type: `text`; text: string }[]
  structuredContent?: unknown
  isError?: true
}

export interface BridgeInput {
  tool: string
  arguments: unknown
  context: BridgeContext
  blobs: BlobStore
  native: (tool: string, args: Record<string, unknown>) => Promise<unknown>
  newId?: () => string
}

export interface BridgeOutput {
  result: McpCallResult
  outcome: HubOutcome
}

export const toolResult = (structuredContent: unknown): McpCallResult => ({ content: [{ type: `text`, text: JSON.stringify(structuredContent) }], structuredContent })
export const toolError = (text: string): McpCallResult => ({ content: [{ type: `text`, text }], isError: true })

class BridgeError extends Error {
  constructor(
    readonly reason: string,
    readonly outcome: HubOutcome,
  ) {
    super(reason)
  }
}

const utf8 = (s: string) => new TextEncoder().encode(s)

async function appFile(input: BridgeInput, id: string): Promise<Uint8Array> {
  const bytes = await input.blobs.get(input.context.app, id)
  if (!bytes) throw new BridgeError(`notFound: ${id}`, `isError`)
  return bytes
}

/** The validated arguments → the native's arguments. */
async function nativeArguments(input: BridgeInput, args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const { app } = input.context
  switch (input.tool) {
    case `files_save`: {
      const name = args.name as string
      const isText = typeof args.text === `string`
      const bytes = isText ? utf8(args.text as string) : await appFile(input, args.fileId as string)
      return { name, mimeType: mimeFor(name, bytes, args.mimeType as string | undefined, isText), data: base64Encode(bytes) }
    }
    case `share_send`: {
      const out: Record<string, unknown> = {}
      for (const key of [`title`, `text`, `url`]) if (key in args) out[key] = args[key]
      if (typeof args.fileId === `string`) {
        const bytes = await appFile(input, args.fileId)
        const name = args.name as string
        out.file = { name, mimeType: mimeFor(name, bytes, args.mimeType as string | undefined, false), data: base64Encode(bytes) }
      }
      return out
    }
    case `print_print`: {
      const bytes = await appFile(input, args.fileId as string)
      const mimeType = sniffMime(bytes)
      const ext = mimeType === `application/pdf` ? `pdf` : mimeType === `image/png` ? `png` : mimeType === `image/jpeg` ? `jpg` : null
      if (!ext) throw new BridgeError(`unsupportedFile`, `isError`)
      return { name: (args.name as string | undefined) ?? `document.${ext}`, mimeType, data: base64Encode(bytes) }
    }
    case `notifications_post`: {
      const screen = args.screen as string | undefined
      if (screen !== undefined && !input.context.screens.includes(screen)) throw new BridgeError(`invalidParams: /screen enum`, `invalidParams`)
      return {
        id: `${app}/${args.id as string}`,
        title: args.title,
        body: args.body ?? null,
        at: args.at ?? null,
        tap: screen === undefined ? null : { type: `notificationTap`, app, screen, params: args.params ?? {} },
      }
    }
    case `notifications_cancel`:
      return { id: `${app}/${args.id as string}` }
    default:
      return args
  }
}

/** Stores every object carrying `data` (depth first, in order): it loses `data`, gains `size` and `fileId`. */
async function storeFiles(input: BridgeInput, value: unknown): Promise<unknown> {
  if (Array.isArray(value)) {
    const out: unknown[] = []
    for (const item of value) out.push(await storeFiles(input, item))
    return out
  }
  if (!isObject(value)) return value
  let out: Record<string, unknown> = {}
  let bytes: Uint8Array | null = null
  if (typeof value.data === `string`) {
    bytes = base64Decode(value.data)
    if (!bytes) throw new BridgeError(`badResult: data format`, `isError`)
  }
  const fileId = bytes ? await input.blobs.put(input.context.app, value.name as string, value.mimeType as string, bytes) : null
  for (const [key, v] of Object.entries(value)) if (!(bytes && key === `data`)) out[key] = await storeFiles(input, v)
  if (bytes) out = { ...out, size: bytes.length, fileId }
  return out
}

/** One capability call from the broker's `call` verdict to its answer. */
export async function bridgeCall(input: BridgeInput): Promise<BridgeOutput> {
  const validated = validateToolArguments(input.tool, input.arguments)
  if (`error` in validated) return { result: toolError(validated.error), outcome: validated.error === `unknownTool` ? `unknown` : `invalidParams` }
  const def = toolDef(input.tool) as CapabilityTool
  const args = validated.arguments
  try {
    if (input.tool === `print_pdf`) {
      const rendered = await renderPdf(args as never, (id) => input.blobs.get(input.context.app, id))
      if (`error` in rendered) return { result: toolError(rendered.error), outcome: `isError` }
      const name = (args.name as string | undefined) ?? pdfFileName(args.title as string)
      const fileId = await input.blobs.put(input.context.app, name, `application/pdf`, rendered.bytes)
      return { result: toolResult({ file: { name, mimeType: `application/pdf`, size: rendered.bytes.length, fileId }, pages: rendered.pages }), outcome: `ok` }
    }
    if (input.tool === `notifications_post` && args.id === undefined) args.id = (input.newId ?? newNotificationId)()
    const nativeArgs = await nativeArguments(input, args)
    let answer: unknown
    try {
      answer = await input.native(input.tool, nativeArgs)
    } catch (error) {
      return { result: toolError(`nativeFailed: ${error instanceof Error ? error.message : String(error)}`), outcome: `rpcError` }
    }
    if (isObject(answer) && answer.isError === true) return { result: answer as unknown as McpCallResult, outcome: `isError` }
    const structured = isObject(answer) ? answer.structuredContent : undefined
    const violation = schemaViolation(def.nativeOutputSchema as JsonSchema, structured)
    if (violation) return { result: toolError(`badResult: ${violation}`), outcome: `isError` }
    if (input.tool === `notifications_post`) return { result: toolResult({ id: args.id }), outcome: `ok` }
    return { result: toolResult(await storeFiles(input, structured)), outcome: `ok` }
  } catch (error) {
    if (error instanceof BridgeError) return { result: toolError(error.reason), outcome: error.outcome }
    throw error
  }
}

/** print.pdf's default file name: the title with `/\:*?"<>|` and controls as `-`, + `.pdf`. */
export const pdfFileName = (title: string): string => `${title.replace(/[/\\:*?"<>|\u0000-\u001f\u007f-\u009f]/g, `-`)}.pdf`
