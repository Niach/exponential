// @exponential_app/eui/web — FEED-128: the FRAMEWORK-FREE web layer every web
// renderer (React, Solid) shares: the theme → CSS compiler (+ the base sheet
// and the per-node box sheet), the data model and bind pass, the URL/media
// policy plumbing, the host plugin types, and the pure helpers natives need
// (coercions, table sort, calendar math, locale numbers, the host-owned
// input controller, the media cache, the markdown parser, the form
// registry, list measurements, listbox arithmetic). Nothing here imports a
// UI framework. The DOM contract: `.xui-surface` roots, `data-xui-id` /
// `data-xui-part` / `data-r-*` / `data-m-*` / `data-xs` attributes,
// `xui-<Component>-<part>` classes, the `LAYERS` cascade order.

export { compileTheme, compiledTheme, globalsCss, themeScope, partClass, propAttribute, tokenVar, tokenVarName, cssValue, declarations, animationDeclarations, shimmerRule, rule, ruleSelectors, isMacroComponent, STATE_SELECTORS, LAYERS } from "./theme-css"
export type { CompiledTheme } from "./theme-css"
export { nodeSheet, compileNodeSheet, nodeSheetBatches, dynamicStyleEntries, queryToken, nodeClass, surfaceClass, walkNodes, cssEscape, resolvedMediaKey, styleDirection } from "./box-css"
export type { NodeSheet, NodeSheetBatch, DynamicStyleEntry } from "./box-css"
export { BASE_CSS } from "./base-css"
export { getPointer, setPointer, pointerTokens, absolutePath, resolveValue, resolveProps, resolveNodeProps, resolveVisible, resolveAccessibility, ariaAttributes, bindTree, boundPath, hostPath, isBinding, isCall, hasDynamic, memoFormatter, formatterOf, viewOf, CLIENT_FUNCTIONS, LITERAL_ROWS_ROOT } from "./data"
export type { DataModel, DataBinding, FunctionCall, ClientFunction, ResolveContext } from "./data"
export { linkHref, mediaRequestOf, mediaUrlAllowed, openAllowed, ownPrivateHosts, urlPolicyOf } from "./urls"
export { lookupIcon } from "./host-types"
export { CUSTOM_ICON_NODES } from "./custom-icons.generated"
export type { CustomIconNode } from "./custom-icons.generated"
export type { HostPlugin, AnyHostPlugin, IconMap, PaintError, SourceOption, SurfaceActionEvent, SurfaceInputEvent } from "./host-types"
export { str, num, bool, arr, objects, phrase, formatFileSize, acceptsFile, failingChecks, failingCheck, builtinIconName, joinIds } from "./coerce"
export { sortRows } from "./table"
export type { TableColumn, TableSort } from "./table"
export { isoDate, parseIsoDate, addDays, addMonths, sameDay, monthGrid, calendarStop } from "./dates"
export { DEBOUNCE_MS, HostOwnedController, parseLocaleNumber } from "./inputs"
export type { HostOwnedControllerOptions, HostOwnedSend } from "./inputs"
export { MEDIA_CACHE_IDLE_MAX, acquireMedia, releaseMedia, mediaCacheKey, mediaCacheStats, clearMediaCache, openMediaSource, streamEntry, fetchStream, fetchLimited } from "./media"
export type { MediaKind, MediaCacheEntry, MediaSourcePlan } from "./media"
export { MAX_LINK_NESTING, MAX_LINK_DEST, MAX_BLOCK_NESTING, LIST_ITEM, linkAt, wholeImage, parseInline, parseList, parseMarkdown } from "./markdown"
export type { MarkdownInline, MarkdownBlock, MarkdownList, MarkdownListItem } from "./markdown"
export { FormRegistry, documentOrder, fieldErrorsVisible, validateOnDefault } from "./form"
export type { FormField, FormError, FormCheck } from "./form"
export { scrollParent, listViewport } from "./list"
export type { ListAxis } from "./list"
export { layoutSize, visualScale, elementScale } from "./primitives/measure"
export type { LayoutSize } from "./primitives/measure"
export { nextEnabledIndex, typeaheadStep, TYPEAHEAD_RESET_MS } from "./listbox"
export type { TypeaheadBuffer } from "./listbox"
export { PLACED, placedPartId } from "./parts"
export { share } from "./share"
// NEW-77: the VoiceWave natives' runtime helpers (the published web entry; the primitives barrel would pull every recipe into the standalone bundle).
export { voiceWaveAnimates, voiceWaveClock, voiceWaveFitChild, voiceWaveName } from "./primitives/voice-wave"
// VAPP-162: the round-5 natives' shared halves (QrCode paint, the QrScanner host hook, the count badge, the Button hold machine).
export { qrPaint, qrScanReporter, qrScanFormats, qrCornerPath, qrFeedColour, QR_CORNERS, QR_SCANNER, segmentedBadge, ButtonHold, HOLD_SLOP, withinSlop } from "./round5"
export type { QrPaint, QrScannerHost, QrCorner, HoldEvent, HoldRect } from "./round5"
