// @exponential-at/ui — the Exponential UI catalog package (VAPP-85).
export * from "./types"
export {
  CORE_CATALOG_ID,
  CORE_LITE_CATALOG_ID,
  A2UI_BASIC_CATALOG_ID,
  A2UI_VERSION,
  SUPPORTED_CATALOG_IDS,
  UNKNOWN_COMPONENT,
  TOKEN_GROUPS,
  coreCatalog,
  coreMacros,
  catalogView,
  componentDef,
  componentNames,
  coreLite,
  parseTokenRef,
  isKnownToken,
} from "./catalog"
export type { CatalogView } from "./catalog"
export * from "./catalog.generated"
export { STYLE_KEYS, STYLE_LAYOUT_KEYS, STYLE_VISUAL_KEYS, STYLE_MEDIA, STYLE_STATES, validateStyle, create, props } from "./style"
export type { Style, StyleProps, MediaKey, StyleIssue } from "./style"
export { validateProps, isDynamic } from "./validate"
export type { PropIssue } from "./validate"
export { evalValue, evalExpr, evalCondition, truthy } from "./expr"
export type { ExprContext } from "./expr"
export { expandMacros } from "./macros"
export { basicMap, mapBasicComponent, mapBasicIcon, BASIC_TRANSFORM_NAMES } from "./basic-map"
export type { MappedComponent } from "./basic-map"
export { reduceSurface, reduceNested, preorder } from "./reducer"
export type { ReduceOptions, ReduceResult } from "./reducer"
export { defineExtension, validateExtension } from "./extension"
export { coreSchema, extensionSchema } from "./schema"
export { catalogPrompt, estimateTokens, PROMPT_RULES, CHARS_PER_TOKEN } from "./prompt"
export type { PromptOptions } from "./prompt"
