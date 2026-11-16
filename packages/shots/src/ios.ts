/**
 * The pure half of the iOS lanes' native-library step (EXP-1558, EXP-1839).
 * `capture-all.ts` runs the build and tuist; this decides, from what is on
 * disk, whether the build has to run first.
 *
 * #PERF-11: the app links ONE Rust library, `ExponentialMobileFFI.xcframework`
 * (genUI, the Engine and P2P), into ExpCore. `exponential-mobile-ffi/build-ios.sh`
 * writes it AND header-only link stubs where the `ExponentialUI` and
 * `ExponentialEngine` Swift packages look for their own xcframework. Project.swift
 * links each only when it exists (`hasEnetNode`, `hasExponentialUi`,
 * `hasEngine`), so a missing one never fails a build: the app paints "Preview"
 * boxes and "No live transcript" instead. A facade's own `build-ios.sh` puts a
 * standalone library back over its stub, and the app then links that code
 * twice; that counts as missing too.
 */
import { join } from "node:path"

export interface IosFfiFramework {
  label: string
  xcframework: string
  /** A link stub (`LINK_STUB` marker), never the facade's standalone library. */
  stub: boolean
}

export const IOS_MOBILE_FFI = {
  script: `apps/desktop/crates/exponential-mobile-ffi/build-ios.sh`,
  /** The rust targets to build (`build-ios.sh` fills only their slices). */
  targetsEnv: `MOBILE_APPLE_TARGETS`,
  frameworks: [
    {
      label: `ExponentialMobileFFI (genUI + Engine + P2P)`,
      xcframework: `apps/desktop/crates/exponential-mobile-ffi/out/ExponentialMobileFFI.xcframework`,
      stub: false,
    },
    {
      label: `ExponentialUIFFI link stub`,
      xcframework: `packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework`,
      stub: true,
    },
    {
      label: `ExponentialEngineFFI link stub`,
      xcframework: `apps/desktop/crates/eengine-ffi/out/ExponentialEngineFFI.xcframework`,
      stub: true,
    },
  ] satisfies IosFfiFramework[],
} as const

/** The simulator slice for a Mac's arch (`process.arch`). */
export function iosSimArch(arch: string): { rust: string; slice: string } {
  return arch === `arm64`
    ? { rust: `aarch64-apple-ios-sim`, slice: `arm64` }
    : { rust: `x86_64-apple-ios`, slice: `x86_64` }
}

export interface IosFfiInputs {
  root: string
  arch: string
  exists: (path: string) => boolean
  /** Directory entries, `[]` for a missing directory. */
  list: (path: string) => string[]
}

/**
 * Why the mobile FFI must be (re)built before the app is generated, one line
 * per framework; empty = everything this Mac's simulator links is in place.
 */
export function iosFfiProblems(inputs: IosFfiInputs): string[] {
  const { slice } = iosSimArch(inputs.arch)
  const problems: string[] = []
  for (const framework of IOS_MOBILE_FFI.frameworks) {
    const dir = join(inputs.root, framework.xcframework)
    if (!inputs.exists(dir)) {
      problems.push(`${framework.xcframework} is missing`)
      continue
    }
    const sim = inputs.list(dir).some((entry) => /^ios-.*-simulator$/.test(entry) && entry.includes(slice))
    if (!sim) {
      problems.push(`${framework.xcframework} has no ${slice} simulator slice`)
      continue
    }
    if (framework.stub && !inputs.exists(join(dir, `LINK_STUB`))) {
      problems.push(`${framework.xcframework} is the facade's standalone library, not the mobile FFI's link stub`)
    }
  }
  return problems
}

/**
 * What the generated project must carry once the mobile FFI exists (tuist
 * caches the evaluated manifests, so a project generated before the
 * xcframeworks existed silently links without them). Empty = fine.
 */
export function iosProjectMissing(pbxproj: string): string[] {
  return [
    pbxproj.includes(`ENET_NODE`) ? undefined : `ENET_NODE (ExpCore without the mobile FFI)`,
    pbxproj.includes(`ExponentialUIWrapper`) ? undefined : `ExponentialUI (the app without the genUI painter)`,
    pbxproj.includes(`ExponentialEngine`) ? undefined : `ExponentialEngine (the app without the Engine)`,
  ].filter((line): line is string => line !== undefined)
}
