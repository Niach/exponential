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
export {
  STYLE_KEYS,
  STYLE_LAYOUT_KEYS,
  STYLE_VISUAL_KEYS,
  STYLE_LAYOUT_EFFECT_KEYS,
  STYLE_MEDIA,
  STYLE_STATES,
  STYLE_TRANSFORM,
  validateStyle,
  create,
  props,
  parseMediaCondition,
  mediaMatches,
  resolveConditions,
  activeBreakpoint,
  styleKeyEnum,
  styleKeyType,
} from "./style"
export type { Style, StyleProps, MediaKey, StateKey, StyleIssue, Gradient, ConditionContext, MediaCondition } from "./style"
export { validateProps, isDynamic } from "./validate"
export type { PropIssue } from "./validate"
export { evalValue, evalExpr, evalCondition, evalConditionValue, truthy, isBinding, isCall, percent, valuesEqual, clampNumber, fillTemplate, CORE_FUNCTIONS, CORE_FUNCTION_NAMES } from "./expr"
export type { ExprContext, Dynamic } from "./expr"
export { expandMacros, isResponsiveValue, responsiveAt, propsAt, BREAKPOINTS } from "./macros"
// Round 1: bind-time evaluation, built-in strings, locale, the CodeBlock tokenizer.
export { resolveDynamic, isVisible, runAction, bindTree, readPointer, writePointer, absolutePath, readPath, hasItem, isDataSchema, resolveProp, resolveNodeProps, hasRowSlots, rowScope, bindRowSlot, BIND_FUNCTIONS, BIND_FUNCTION_NAMES } from "./dynamic"
export type { DataScope, ResolveOptions, ActionOutcome, FunctionTable } from "./dynamic"
export { DEFAULT_STRINGS, STRING_IDS, parseStringRef, isStringRef, stringTable, formatString, resolveString } from "./strings"
export { DEFAULT_LOCALE, parseLocale, localeRegion, weekStart, textDirection, RTL_MIRRORED_ICONS, mirrorsInRtl } from "./locale"
export type { LocaleParts } from "./locale"
export { tokenizeCode, CODE_LANGUAGES, CODE_LANGUAGE_NAMES } from "./code"
export { A11Y_ROLES, A11Y_RULES, A11Y_COMMANDS, COMPONENT_A11Y, componentA11y, isA11yRole } from "./a11y"
export { chartExtent, chartSummaryParams, sparklineSummaryParams, niceTicks, seriesColor, DONUT_HOLE, WINDOW_THRESHOLD } from "./chart"
export type { ChartSeries } from "./chart"
export type { ComponentA11y } from "./a11y"
export type { CodeToken } from "./code"
export { basicMap, mapBasicComponent, mapBasicIcon, BASIC_TRANSFORM_NAMES } from "./basic-map"
export type { MappedComponent } from "./basic-map"
export { reduceSurface, reduceNested, preorder, RESERVED_KEYS, validVisible, slotAllowed, childTemplate } from "./reducer"
export type { ReduceOptions, ReduceResult } from "./reducer"
export { defineExtension, validateExtension } from "./extension"
export { coreSchema, extensionSchema } from "./schema"
export { catalogPrompt, estimateTokens, PROMPT_RULES, CHARS_PER_TOKEN } from "./prompt"
export type { PromptOptions } from "./prompt"
// VAPP-92: themes
export * from "./theme-types"
export { parseColor, toHex, toThemeHex, isThemeHex, luminance, contrast } from "./color"
export type { Rgba } from "./color"
export { RECIPE_STATES, RECIPE_KEYS, recipeParts, macroParts, nativeRecipeProps } from "./recipes"
export type { PartSpec } from "./recipes"
export {
  MODES,
  DENSITIES,
  THEME_SCHEMA_ID,
  resolveMode,
  applyDensity,
  applyContrast,
  resolveConditionKey,
  easingCss,
  ThemeError,
  validateTheme,
  tryLoadTheme,
  loadTheme,
  resolveToken,
  resolveStyleValues,
  recipeStyle,
  resolveRecipe,
  nodeRecipeQuery,
  resolveNodeStyle,
  shadowCss,
} from "./theme"
export type { ThemeOptions } from "./theme"
export { BUILTIN_THEMES, BUILTIN_THEME_IDS, DEFAULT_THEME_ID, neutralTheme, exponentialTheme, playfulTheme, builtinTheme, builtinThemes } from "./themes"
export { styleToCss, styleAttribute, gradientCss } from "./css"
export { SHADCN_COLOR_VARS, importShadcnCss, themeFromImport, diffTheme, exportThemeJson, parseThemeJson, parseShadow } from "./builder"
export type { ThemeImport } from "./builder"
export { CONTROL_PARTS, GEOMETRY_KEYS, controlGeometry, checkGeometry, verifyPainterGeometry } from "./geometry"
export type { ControlGeometry, GeometryKey, MeasuredBox, PainterOverride, GeometryIssue } from "./geometry"
// VAPP-87: the overlay placement contract (renderers + the Rust core).
export { OVERLAY_OFFSET, OVERLAY_PADDING, placeOverlay } from "./overlay"
export type { OverlaySide, OverlayAlign, OverlayPlacement, Rect, Size } from "./overlay"
// VAPP-91: the host API (transport, functions, bindings, negotiation, policy, packages).
export * from "./host"
// VAPP-91: the conformance suite.
export { CONFORMANCE_VERSION, conformanceManifest, checkReport } from "./conformance"
export type { ConformanceSuite, ConformanceManifest, ConformanceReport, ReportVerdict } from "./conformance"
// VAPP-91: declarative vapps in any host + the Exponential connector.
export * from "./connector"
