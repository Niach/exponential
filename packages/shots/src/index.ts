/**
 * `@exp/shots` — the cross-platform screenshot store (EXP-566).
 *
 * The catalog (`@exp/view-catalog`) says WHICH views exist and how each platform
 * reaches them. This package is the machinery around it: the encode pipeline,
 * the diff-skipping store writer, the fastlane importer, the desktop capturer
 * and the orchestrator that sequences all four lanes against one seeded
 * instance.
 *
 * Entry points are the two CLIs (`src/capture-all.ts`, `src/index-store.ts`);
 * everything a consumer might import is re-exported here.
 */
export {
  encodeShot,
  imageSize,
  luminanceVariance,
  toRawRgba,
  MAX_LONG_EDGE,
  WEBP_OPTIONS,
  type EncodedShot,
  type RawImage,
} from "./encode.ts"

export {
  indexStore,
  readIndex,
  toleranceFor,
  writeShot,
  type IndexEntry,
  type IndexStoreOptions,
  type IndexStoreResult,
  type Orphan,
  type ShotState,
  type StoreIndex,
  type WriteShotOptions,
  type WriteShotResult,
} from "./store.ts"

export {
  indexFileField,
  indexPath,
  rawDir,
  rawShotPath,
  repoRoot,
  storeDir,
  storeShotPath,
} from "./paths.ts"

export {
  importNative,
  NATIVE_PLATFORMS,
  STORE_FRAME_MARKER,
  type ImportedShot,
  type ImportNativeOptions,
  type ImportNativeResult,
} from "./import-native.ts"

export {
  captureDesktop,
  captureManual,
  driveEnv,
  mintSessionToken,
  resolveBinary,
  screenRecordingAllowed,
  DEFAULT_ANCHOR_DELAY_MS,
  DEFAULT_BINARY,
  DEFAULT_PROCESS_NAME,
  WINDOW_SIZE,
  type CaptureDesktopOptions,
  type CaptureDesktopResult,
  type DesktopShotResult,
} from "./capture-desktop.ts"

export {
  fetchDemoIds,
  missingPlaceholder,
  parseDemoIds,
  resolveDriveValue,
  type DemoIds,
} from "./ids.ts"
