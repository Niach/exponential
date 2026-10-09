// VAPP-103: the input limits (catalog/limits.json), one place for the TS
// reference and the generated constants of every target. Past a limit the
// core reports an issue and does no work.

import limitsJson from "../catalog/limits.json" with { type: "json" }

export const LIMITS: Readonly<Record<string, number>> = Object.fromEntries(Object.entries(limitsJson).filter(([k]) => !k.startsWith(`$`))) as Record<string, number>
/** Nodes the reducer places in one surface's tree (template nodes included). */
export const MAX_COMPONENTS: number = limitsJson.maxComponents
/** Component nesting levels (the root = 1). */
export const MAX_DEPTH: number = limitsJson.maxDepth
/** One server message as UTF-8 JSON. */
export const MAX_MESSAGE_BYTES: number = limitsJson.maxMessageBytes
/** Template items one surface instantiates. */
export const MAX_TEMPLATE_ITEMS: number = limitsJson.maxTemplateItems
/** A data pointer's length in UTF-8 bytes. */
export const MAX_POINTER_BYTES: number = limitsJson.maxPointerBytes
/** A data pointer's token count. */
export const MAX_POINTER_SEGMENTS: number = limitsJson.maxPointerSegments

/** The issue messages, byte-identical in every core. */
export const LIMIT_ISSUES = {
  usedTwice: `id used twice; only its first place renders`,
  depth: `nesting deeper than ${MAX_DEPTH} levels`,
  components: `surface: more than ${MAX_COMPONENTS} components; the rest is dropped`,
  templateItems: `template: more than ${MAX_TEMPLATE_ITEMS} items; the rest is not rendered`,
  messageBytes: `message larger than ${MAX_MESSAGE_BYTES} bytes`,
} as const
