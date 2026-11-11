// @exponential_app/eui — the Exponential UI catalog package (VAPP-85).
export * from "./types"
export {
  CORE_CATALOG_ID,
  CORE_LITE_CATALOG_ID,
  A2UI_BASIC_CATALOG_ID,
  A2UI_VERSION,
  ACCEPT_V09,
  SUPPORTED_CATALOG_IDS,
  UNKNOWN_COMPONENT,
  TOKEN_GROUPS,
  coreCatalog,
  coreMacros,
  catalogView,
  sharedCatalogView,
  componentDef,
  componentNames,
  coreLite,
  isOffered,
  parseTokenRef,
  isKnownToken,
} from "./catalog"
export type { CatalogView } from "./catalog"
export * from "./catalog.generated"
export { newSurfaceBridge, surfaceUpdates } from "./surface-updates"
export type { SurfaceBridge, SurfaceRefusal, SurfaceUpdate, SurfaceUpdatesOptions } from "./surface-updates"
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
  formFactorOf,
  DEFAULT_FORM_FACTOR_BREAKPOINT,
  styleKeyEnum,
  styleKeyType,
} from "./style"
export type { Style, StyleProps, MediaKey, StateKey, StyleIssue, Gradient, ConditionContext, MediaCondition, FormFactor } from "./style"
export { validateProps, validateNode, isCallableName, isKnownFunctionName, isDynamic } from "./validate"
export type { PropIssue } from "./validate"
export { evalValue, evalExpr, evalCondition, evalConditionValue, truthy, present, coerceBoolean, binding, bindingPath, callName, isBinding, isCall, percent, valuesEqual, clampNumber, fillTemplate, filterItems, CORE_FUNCTIONS, CORE_FUNCTION_NAMES } from "./expr"
export type { ExprContext, Dynamic } from "./expr"
export { DATE_BUCKETS, dateBucket, parseDay, parseMonth, parseYear, formatDay, isoWeek, isoWeeksInYear, dayOfYear, daysInMonth, weekdayOf, isLeapYear, dayNumber, dayFromNumber } from "./dates"
export { EXPRESSION_LIMITS, EXPRESSION_FUNCTIONS, EXPRESSION_CALL_NAMES, EXPRESSION_BIND_FUNCTIONS, DATE_UNITS, CALENDAR_UNITS, DURATION_FORMATS, parseExpression, evaluateExpression, checkExpression, callExpressionFunction, parseTimestamp, formatTimestamp, typeName as expressionTypeName, typeDisplay } from "./cel"
export type { ExprType, ExprNode, ExpressionEnv, ExpressionResult } from "./cel"
export type { CalendarDay, DateBucket } from "./dates"

export { expandMacros, isResponsiveValue, responsiveAt, propsAt, BREAKPOINTS } from "./macros"
// Round 1: bind-time evaluation, built-in strings, locale, the CodeBlock tokenizer.
export { resolveDynamic, isVisible, runAction, withOwnWrites, submitClosesOverlay, bindTree, readPointer, writePointer, writeTokens, POINTER_ISSUES, absolutePath, readPath, hasItem, isDataSchema, resolveProp, resolveNodeProps, hasRowSlots, rowScope, bindRowSlot, sectionScope, bindSectionHeader, BIND_FUNCTIONS, BIND_FUNCTION_NAMES, EFFECT_FUNCTIONS, USER_ACTIVATION_FUNCTIONS, RENDERER_ONLY_FUNCTIONS, VALIDATOR_FUNCTIONS, FunctionFailure, catalogFunctionNames, checkResult, evaluateCall, evaluateCheck, fieldChecks, isHostFunctionName, resolveWithFailure, shownResult } from "./dynamic"
export { allowedCallers, checkArgs, declaredFunction, INDEX_FUNCTION, requiresUserActivation } from "./functions"
export type { AllowedCallers } from "./functions"
export { A2UI_VERSION_09, ENVELOPE_KEYS, SURFACE_COMPONENT, checkCall, checkDynamicValue, checkEntry, checkEnvelope, checkMetadata, entryRefusal } from "./entry"
export type { EntryContext, EntryFailure, Envelope } from "./entry"
// Round 2 (docs/round-2-contract.md): formatting, lists, panels, direction, animation.
export { displayString, englishFormatter, intlFormatter, formatCompact, formatFunctions, formatPattern, parseDateValue, relativeTimeUnit, currencyDigits, FORMAT_FUNCTION_NAMES, ENGLISH_FORMAT_FUNCTIONS, ENGLISH_DATE_PATTERNS, ENGLISH_TIME_PATTERN, ENGLISH_DATE_TIME_JOIN, fixedOffset, intlZoneOffset } from "./format"
export type { Formatter, NumberOptions, CompactOptions, DateOptions, DateStyle, PluralCategory, RelativeUnit, ZoneOffset } from "./format"
export { templateItemKeys, tableRowKeys, templateInstances, itemExtents, itemOffsets, virtualWindow, scrollOffsetForIndex, scrollOffsetForItem, instanceSegment, listSections, sectionRows, stickyHeader, templateBudget, templateSiteKey, WINDOW_OVERSCAN } from "./list"
export type { TemplateInstance, TemplateBudget, WindowRange, ScrollAlign, ListSection } from "./list"
export {
  ZOOM,
  zoomIsTall,
  zoomFitScale,
  zoomLimits,
  zoomRestScale,
  zoomRestView,
  zoomClamp,
  zoomAtRest,
  zoomTo,
  zoomCentre,
  zoomWheelFactor,
  zoomWheel,
  zoomScroll,
  zoomLadder,
  zoomStepScale,
  zoomStep,
  zoomDoubleTapScale,
  zoomDoubleTap,
  zoomKey,
  zoomRubberBand,
  zoomDrag,
  zoomPinch,
  zoomMomentum,
  zoomMomentumActive,
  zoomPanRoom,
  zoomResize,
  zoomPercent,
  zoomMenu,
  zoomBounds,
} from "./zoom"
export type { ZoomFrame, ZoomInitial, ZoomMenuItem, ZoomPoint, ZoomSize, ZoomView, WheelDeltaMode } from "./zoom"
export { normalizeSizes, resizePanels, keyboardResize, panelExtents, dragDelta, RESIZE_STEP, PANEL_MIN, RESIZE_HANDLE_HIT } from "./resizable"
export type { PanelLimits, ResizeKey } from "./resizable"
export { partitionTabs, tabDropSlot, tabKeySlot, tabReorder, moveTab } from "./tabs-strip"
export { SWIPE, swipeBlocks, swipeIndicator, swipeLock, swipeOffset, swipeProgress, swipeSettle, swipeStep, type SwipeLock } from "./swipe"
export { MIDDLE_MIN_KEPT, middleTruncateWidth } from "./text-truncate"
export { nodeDirections, physicalTextAlign, nodeTextAlign } from "./direction"
export type { Direction, PhysicalAlign } from "./direction"
export { LIMITS, MAX_COMPONENTS, MAX_DEPTH, MAX_MESSAGE_BYTES, MAX_TEMPLATE_ITEMS, MAX_POINTER_BYTES, MAX_POINTER_SEGMENTS, LIMIT_ISSUES } from "./limits"
export { LAYOUT_CONSTANTS, FIELD_INTRINSIC_WIDTH, MEDIA_INTRINSIC_WIDTH, MEDIA_ASPECT_RATIO, TREE_GUIDE_COLUMN, TREE_GUIDE_RADIUS, TREE_GUIDE_BRIDGE, PENDING_DELAY_MS, SKELETON_PULSE_MS, SKELETON_OPACITY_LOW, SKELETON_OPACITY_HIGH, SKELETON_REDUCED_OPACITY, SEARCH_LIST_FADE, TAB_STRIP_MAX_WIDTH, TAB_DRAG_THRESHOLD, TAB_LONG_PRESS_MS } from "./layout"
export { treeGuides, applyTreeGuides, isRowRoot } from "./tree-guides"
export { readOnlyTree, readOnlyTemplates, readOnlyDisables, readOnlyDisabledIds, actionWrites, callWrites, READ_ONLY_FIELDS, READ_ONLY_WRITE_PREFIXES } from "./read-only"
export type { TreeGuide } from "./tree-guides"
export { ANIMATIONS, ANIMATION_NAMES, ANIMATION_PROPERTIES_CSS, animatesOpacity, animationTiming, animationFrame, cubicBezier, keyframesCss, animationCss, paintedOpacity } from "./animation"
export type { AnimationDef, AnimationFrame, AnimationTiming, Keyframe } from "./animation"
export type { DataScope, FieldChecks, ResolveOptions, ActionOutcome, Failure, FunctionTable, PointerWrite, RouteAnswer, RoutedCall, ValidationResult } from "./dynamic"
export { DEFAULT_STRINGS, STRING_IDS, parseStringRef, isStringRef, stringTable, formatString, resolveString } from "./strings"
export { DEFAULT_LOCALE, parseLocale, localeRegion, weekStart, textDirection, RTL_MIRRORED_ICONS, mirrorsInRtl } from "./locale"
export type { LocaleParts } from "./locale"
export { tokenizeCode, markdownCodeLanguage, codeText, ansiRuns, CODE_LANGUAGES, CODE_LANGUAGE_NAMES } from "./code"
export type { AnsiRun } from "./code"
export { textClusters, trackedClusters } from "./text-clusters"
export { textShimmer, TEXT_SHIMMER } from "./text-shimmer"
export type { TextShimmer } from "./text-shimmer"
export { voiceWave, voiceWavePath, voiceWavePhase, voiceWaveChildSize, voiceWaveChildBox, voiceWaveActivity, VOICE_WAVE, VOICE_WAVE_ACTIVITIES, VOICE_WAVE_RINGS } from "./voice-wave"
export type { VoiceWave, VoiceWaveActivity, VoiceWaveChildBox, VoiceWaveInput, VoiceWaveRing, VoiceWaveRingName } from "./voice-wave"
export { voiceWaveBars, voiceWaveBarsBox, voiceWaveVariant, VOICE_WAVE_BARS, VOICE_WAVE_SIZES, VOICE_WAVE_VARIANTS } from "./voice-wave"
export type { VoiceWaveBar, VoiceWaveBars, VoiceWaveBarsInput, VoiceWaveSizeName, VoiceWaveVariant } from "./voice-wave"
export { qrMatrix, qrLayout, qrLevel, qrVersion, QR_CODE, QR_LEVELS } from "./qr-code"
export type { QrLevel, QrMatrix } from "./qr-code"
export { A11Y_ROLES, A11Y_RULES, A11Y_COMMANDS, COMPONENT_A11Y, componentA11y, isA11yRole } from "./a11y"
export { chartData, chartExtent, chartSummaryParams, formatChartValue, sparklineSummaryParams, niceTicks, seriesColor, DONUT_HOLE, WINDOW_THRESHOLD } from "./chart"
export type { ChartSeries, ChartValueFormat } from "./chart"
export type { ComponentA11y } from "./a11y"
export type { CodeToken } from "./code"
export { basicMap, mapBasicComponent, mapBasicIcon, BASIC_TRANSFORM_NAMES } from "./basic-map"
export type { MappedComponent } from "./basic-map"
export { reduceSurface, reduceNested, cardV10, cardProblems, preorder, RESERVED_KEYS, validVisible, slotAllowed, childTemplate } from "./reducer"
export type { ReduceOptions, ReduceResult } from "./reducer"
export { defineExtension, validateExtension } from "./extension"
// VAPP-106: NOT here, each its own entry so the root stays what a renderer
// needs: `@exponential_app/eui/prompt` (catalogPrompt, estimateTokens,
// PROMPT_RULES, the described catalog), `./schema` (coreSchema,
// extensionSchema + the vendored A2UI basic catalog), `./conformance` (the
// suite + every fixture), `./host` (the host API, also re-exported below).
// VAPP-108: the typed authoring DSL (defineSurface + the ui factories).
export { defineSurface, flattenSurface, ui, el, bind, call, event, run } from "./define"
export type { SurfaceNode, TemplateChildren, DefineOptions, DefinedSurface, Factory } from "./define"
// VAPP-92: themes
export * from "./theme-types"
export { parseColor, toHex, toThemeHex, isThemeHex, luminance, contrast } from "./color"
export type { Rgba } from "./color"
export { RECIPE_STATES, RECIPE_KEYS, markdownInlineRecipes, recipeParts, macroParts, nativeRecipeProps, rootPartMacro } from "./recipes"
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
export { BUILTIN_THEMES, BUILTIN_THEME_IDS, DEFAULT_THEME_ID, neutralTheme, exponentialTheme, playfulTheme, orangeTheme, builtinTheme, builtinThemes, themeOrDefault } from "./themes"
export { styleToCss, styleAttribute, gradientCss } from "./css"
export { SHADCN_COLOR_VARS, importShadcnCss, themeFromImport, diffTheme, exportThemeJson, parseThemeJson, parseShadow } from "./builder"
export type { ThemeImport } from "./builder"
export { CONTROL_PARTS, GEOMETRY_KEYS, controlGeometry, checkGeometry, verifyPainterGeometry } from "./geometry"
export type { ControlGeometry, GeometryKey, MeasuredBox, PainterOverride, GeometryIssue } from "./geometry"
// VAPP-87: the overlay placement contract (renderers + the Rust core).
export { OVERLAY_OFFSET, OVERLAY_PADDING, anchorPosition, placeInsetOverlay, placeOverlay, snapInset } from "./overlay"
export type { OverlaySide, OverlayAlign, OverlayPlacement, Rect, Size } from "./overlay"
// VAPP-91: the host API (transport, functions, bindings, negotiation, policy, packages).
export * from "./host"
// VAPP-91: declarative apps in any host + the Exponential connector.
export * from "./connector"
// #VAPP-284: the fold keys (a foldable Row's head and fold button).
export { foldKey } from "./fold"
// #VAPP-285: the Tree rules (flat rows, the WAI-ARIA tree keys).
export { treeRows, treeKey } from "./tree"
export type { TreeRow, TreeRowKind, TreeKeyRow, TreeKeyResult } from "./tree"
// Round 8: the Canvas node's pure core and the Checkbox press rule.
export * from "./canvas"
export * from "./checkbox"
export * from "./upgrade09"
export { isIdentifier, mangle } from "./ids"
// #EDE-65: Embed, a sandboxed nested surface (fixtures/embed.json).
export { EMBED_ERROR_ROOT, EMBED_EVENT, embedCard, embedMount, embedPayload, embedSurfaceId } from "./embed"
export type { EmbedCard } from "./embed"
// #EXP-1629: a Menu item's side preview (interactions-round1.json `menuPreview`).
export { MENU_PREVIEW_CLOSE_MS, MENU_PREVIEW_IDLE, MENU_PREVIEW_OFFSET, MENU_PREVIEW_OPEN_MS, menuPreviewId, menuPreviewInput, menuPreviewTick, menuPreviewWanted } from "./menu-preview"
export type { MenuPreviewInput, MenuPreviewState } from "./menu-preview"
