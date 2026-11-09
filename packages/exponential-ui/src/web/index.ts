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
export { QUIET_FOCUS_ATTR, openFocusTarget, overlayOpenFocus, tooltipFocusHost, tooltipPhysicalSide } from "./focus"
export { findAnchor, linkAnchor, markAnchor, virtualAnchor, watchAnchor, withinAnchor, withinOtherOverlay } from "./anchor"
export type { AnchorLink, VirtualAnchor } from "./anchor"
export { HOSTED_POPUP_ATTR, hostedPopupOf, mountHostedPopup } from "./popup-host"
export type { HostedPopup, PopupHost } from "./popup-host"
export { DIALOG_HOLD_ATTR, DIALOG_HOLD_VAR, holdDialogBottom } from "./dialog-hold"
export { getPointer, setPointer, pointerTokens, absolutePath, resolveValue, resolveProps, resolveNodeProps, resolveVisible, resolveAccessibility, ariaAttributes, liveOf, bindTree, boundPath, hostPath, isBinding, isCall, hasDynamic, memoFormatter, formatterOf, viewOf, CLIENT_FUNCTIONS, LITERAL_ROWS_ROOT } from "./data"
export type { DataModel, DataBinding, FunctionCall, ClientFunction, ResolveContext } from "./data"
export { linkedMarkdown } from "./markdown-linked"
export { linkHref, textLinkSegments, mediaRequestOf, mediaUrlAllowed, openAllowed, ownPrivateHosts, urlPolicyOf } from "./urls"
export { lookupIcon } from "./host-types"
export { CUSTOM_ICON_NODES } from "./custom-icons.generated"
export type { CustomIconNode } from "./custom-icons.generated"
export type { HostPlugin, AnyHostPlugin, IconMap, PaintError, SourceOption, SurfaceActionEvent, SurfaceInputEvent, TextLinkSegment, TextLinkChip, MarkdownAttachmentInfo } from "./host-types"
export { str, num, bool, arr, objects, phrase, formatFileSize, mediaClock, acceptsFile, failingChecks, resolvedFieldChecks, failingCheck, builtinIconName, joinIds } from "./coerce"
export { sortRows, tableMinWidth, tableTemplate, tableTotals } from "./table"
export type { TableColumn, TableSort } from "./table"
export { isoDate, parseIsoDate, addDays, addMonths, sameDay, monthGrid, calendarStop, markTones, openingMonth, isoMonth, monthView } from "./dates"
export type { DateMark, MonthCell } from "./dates"
export { DEBOUNCE_MS, HostOwnedController, parseLocaleNumber, switchTravel } from "./inputs"
export type { HostOwnedControllerOptions, HostOwnedSend } from "./inputs"
export { MEDIA_CACHE_IDLE_MAX, acquireMedia, releaseMedia, mediaCacheKey, mediaCacheStats, clearMediaCache, openMediaSource, streamEntry, fetchStream, fetchLimited } from "./media"
export type { MediaKind, MediaCacheEntry, MediaSourcePlan } from "./media"
export { MAX_LINK_NESTING, MAX_LINK_DEST, MAX_BLOCK_NESTING, LIST_ITEM, linkAt, wholeImage, parseInline, parseList, parseMarkdown, resetMarkdownStreamMemo, MARKDOWN_LAZY_BLOCKS, linkMarkdownText, markdownCodeLines, hostMarkdown, markdownImageBox, markdownImageCap, markdownImageStyle, markdownImageSizeOf, loadedImageSize, rememberImageSize, MARKDOWN_UNSIZED_ASPECT, validChip, CHIP_KINDS, ATTACHMENT_KINDS, MARKDOWN_IMAGE_MAX_HEIGHT } from "./markdown"
export { markdownPartAttrs, markdownChipOverlay, markdownProfileHost } from "./markdown-parts"
export type { MarkdownDensity, MarkdownVariant } from "./markdown-parts"
export type { MarkdownInline, MarkdownBlock, MarkdownList, MarkdownListItem, MarkdownChip, MarkdownAttachment, MarkdownHooks, MarkdownOptions } from "./markdown"
export { FormRegistry, documentOrder, fieldErrorsVisible, validateOnDefault } from "./form"
export type { FormField, FormError, FormCheck } from "./form"
export { scrollParent, listViewport } from "./list"
export { stripFit, stripKeyMove, isReorderKey, startStripDrag } from "./tabs-strip"
export type { StripDragSource, StripDragHandlers } from "./tabs-strip"
export { startTabSwipe, swipeBlocked, tabPage, tabPageStyle, tabSpans, sameSpans, tabThumbStyle } from "./tabs-swipe"
export type { TabSwipeDrag, TabSpan, TabSwipeSource, TabSwipeHandlers } from "./tabs-swipe"
export type { ListAxis } from "./list"
export { layoutSize, visualScale, elementScale } from "./primitives/measure"
export type { LayoutSize } from "./primitives/measure"
export { nextEnabledIndex, typeaheadStep, TYPEAHEAD_RESET_MS } from "./listbox"
export type { TypeaheadBuffer } from "./listbox"
export { PLACED, placedPartId } from "./parts"
export { share } from "./share"
// NEW-77: the VoiceWave natives' runtime helpers (the published web entry; the primitives barrel would pull every recipe into the standalone bundle).
export { voiceWaveAnimates, voiceWaveClock, voiceWaveFitChild, voiceWaveName } from "./primitives/voice-wave"
export { MotionView, MotionStateDriver, motionA11y, motionBoxStyle, motionCanPaint, motionDeclaredSize, motionDotLottie, motionFit, motionFromDotLottie, motionThemeSlots, paintMotionFrame, motionFitMatrix, motionConcat, motionFrameSize, MotionCanvasPool, MOTION_MAX_PLAYING, MOTION_MAX_FPS } from "./primitives/motion"
export type { MotionEnv, MotionEmit, MotionEvent, MotionMachinePlayer, MotionFit, MotionMatrix } from "./primitives/motion"
export { CanvasView, canvasCanPaint, canvasThemeFonts, resetCanvasThemeIds } from "./primitives/canvas"
export type { CanvasEnv } from "./primitives/canvas"
export { paintText, MOTION_DEFAULT_FONTS, MOTION_PACKED_VERSION } from "./primitives/motion-paint"
export type { MotionTextFonts } from "./primitives/motion-paint"
// VAPP-162: the round-5 natives' shared halves (QrCode paint, the QrScanner host hook, the count badge, the Button hold machine).
export { qrPaint, qrScanReporter, qrScanFormats, qrCornerPath, qrFeedColour, QR_CORNERS, QR_SCANNER, segmentedBadge, ButtonHold, HOLD_SLOP, withinSlop } from "./round5"
export type { QrPaint, QrScannerHost, QrCorner, HoldEvent, HoldRect } from "./round5"
// #VAPP-285: the Tree keys in the DOM (React + Solid Box).
export { treeKeyTarget } from "./tree-keys"
// #EXP-1629: the KeyRecorder field's DOM capture (React + Solid Box).
export { keyRecorderKey, keyRecorderCode, keyRecorderKeyPayload, keyRecorderMousePayload, isKeyRecorderField } from "./key-recorder"
export type { KeyRecorderPayload } from "./key-recorder"
