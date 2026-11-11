// #VAPP-292 — what the genUI view (`ui://exponential/genui`) is fed. Two
// tools bind it: `exponential_show` outside a session (an inline card) and
// `exponential_apps` (op `get` = a package to run, op `list` = the apps to
// open). Pure: the server builds the show answer with `inlineShowResult`, the
// view decodes every answer with `decodeGenuiResult`.

/** The validated show card (`buildSurfaceCard`'s, kind `surface`). */
export interface InlineCard {
  kind: `surface`
  surface: { root: Record<string, unknown>; data?: Record<string, unknown> | null }
  title?: string | null
}

/** One app row of `exponential_apps {op: "list"}`. */
export interface GenuiAppRow {
  id: string
  slug: string
  name: string
  teamId: string
  icon?: string | null
  color?: string | null
  publishedVersion?: number
  headVersion?: number
}

/** `exponential_apps {op: "get"}`'s answer (the fields the view reads). */
export interface GenuiAppRead {
  id: string
  slug: string
  name: string
  teamId: string
  url?: string | null
  version?: number
  package: Record<string, unknown> | null
  /** The typed actions the app may run without asking (its grants). */
  grantedTools?: string[]
  /** Who reads it and their app access (the view runs it as them). */
  viewer?: { id: string; access: `read` | `write` }
}

export type GenuiScreen =
  | { kind: `card`; card: InlineCard }
  | { kind: `app`; app: GenuiAppRead }
  | { kind: `apps`; apps: GenuiAppRow[] }
  /** Any other apps op (records, catalog…): nothing to draw. */
  | { kind: `none` }
  | { kind: `error`; message: string }

/** The inline show's answer: the card the view paints (the model wrote the
 *  surface, so echoing it costs nothing new). */
export function inlineShowResult(card: InlineCard): { shown: `inline`; card: InlineCard } {
  return { shown: `inline`, card }
}

interface ToolResultLike {
  content?: Array<{ type: string; text?: string }>
  structuredContent?: unknown
  isError?: boolean
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** The JSON a tool answered (structuredContent, else its first text block). */
export function toolJson(result: ToolResultLike): { ok: true; data: unknown } | { ok: false; message: string } {
  const text = result.content?.find((c) => c.type === `text`)?.text
  if (result.isError) return { ok: false, message: text || `The tool failed.` }
  if (result.structuredContent !== undefined) return { ok: true, data: result.structuredContent }
  if (text === undefined) return { ok: true, data: null }
  try {
    return { ok: true, data: JSON.parse(text) }
  } catch {
    return { ok: false, message: text }
  }
}

/** What a bound tool's answer draws. */
export function decodeGenuiResult(result: ToolResultLike): GenuiScreen {
  const decoded = toolJson(result)
  if (!decoded.ok) return { kind: `error`, message: decoded.message }
  const data = decoded.data
  if (isObject(data) && data.shown === `inline` && isObject(data.card) && isObject((data.card as { surface?: unknown }).surface)) {
    return { kind: `card`, card: data.card as unknown as InlineCard }
  }
  if (isObject(data) && typeof data.slug === `string` && typeof data.id === `string` && `package` in data) {
    return { kind: `app`, app: data as unknown as GenuiAppRead }
  }
  if (Array.isArray(data) && data.every((row) => isObject(row) && typeof row.slug === `string` && typeof row.name === `string` && typeof row.id === `string`)) {
    return { kind: `apps`, apps: data as GenuiAppRow[] }
  }
  return { kind: `none` }
}
