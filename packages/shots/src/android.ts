/**
 * The pure halves of the android `package` lane (EXP-1264): where the Android
 * SDK lives, which ABI the attached device runs, and whether a launched app is
 * actually the thing on screen. `capture-all.ts` does the adb/gradle/cargo
 * calls; everything here reads text those calls returned, so it is testable
 * without a device.
 *
 * The bug this exists for: the Compose example app crashes on start when
 * `libexponential_ui_ffi.so` is missing from its apk, the lane photographed the
 * launcher underneath, and the store kept 84 pictures of the home screen as
 * `updated`. A shot is now taken only once the app is alive, focused and has
 * logged its first layout pass.
 */
import { join } from "node:path"

/* ------------------------------------------------------------------ SDK path */

/**
 * `sdk.dir` from a gradle `local.properties`, or undefined. Java properties
 * escape `:` and `\` (Android Studio writes `C\:\\Users\\…` on Windows), so the
 * value is unescaped the way `java.util.Properties` would.
 */
export function sdkDirFromLocalProperties(text: string): string | undefined {
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim()
    if (line.startsWith(`#`) || line.startsWith(`!`)) continue
    const match = /^sdk\.dir\s*[=:]\s*(.*)$/.exec(line)
    if (!match) continue
    const value = match[1]!.replace(/\\(.)/g, `$1`).trim()
    return value === `` ? undefined : value
  }
  return undefined
}

export interface AndroidSdkInputs {
  /** The text of `apps/android/local.properties`, when it exists. */
  localProperties?: string
  env: Record<string, string | undefined>
  home: string
  exists: (path: string) => boolean
}

export interface AndroidSdk {
  path: string
  /** Where the path came from, for the preflight line. */
  source: `apps/android/local.properties` | `ANDROID_HOME` | `ANDROID_SDK_ROOT` | `default`
}

/**
 * The Android SDK the way the app lane finds it: gradle reads
 * `apps/android/local.properties` there, so that wins; then `ANDROID_HOME`
 * (and its deprecated twin `ANDROID_SDK_ROOT`); then Android Studio's default
 * `~/Library/Android/sdk`. A candidate that does not exist on disk is skipped,
 * so a stale `local.properties` from another machine does not shadow a good env.
 */
export function resolveAndroidSdk(inputs: AndroidSdkInputs): AndroidSdk | undefined {
  const candidates: [string | undefined, AndroidSdk[`source`]][] = [
    [
      inputs.localProperties === undefined ? undefined : sdkDirFromLocalProperties(inputs.localProperties),
      `apps/android/local.properties`,
    ],
    [inputs.env.ANDROID_HOME, `ANDROID_HOME`],
    [inputs.env.ANDROID_SDK_ROOT, `ANDROID_SDK_ROOT`],
    [join(inputs.home, `Library/Android/sdk`), `default`],
  ]
  for (const [path, source] of candidates) {
    if (path && inputs.exists(path)) return { path, source }
  }
  return undefined
}

/* ----------------------------------------------------------------------- ABI */

/** The ABIs `build-android.sh` (cargo-ndk) can build the FFI for. */
export const FFI_ABIS = [`arm64-v8a`, `armeabi-v7a`, `x86_64`] as const
export type FfiAbi = (typeof FFI_ABIS)[number]

/**
 * The FFI ABI for a device, from `getprop ro.product.cpu.abilist` (or the
 * single `ro.product.cpu.abi`): the first ABI the device prefers that the
 * script can build. An Apple-silicon emulator says `arm64-v8a`; an x86_64
 * image lists `x86_64,arm64-v8a` (translation) and gets the native one.
 */
export function deviceFfiAbi(abilist: string): FfiAbi | undefined {
  for (const abi of abilist.split(/[\s,]+/)) {
    if ((FFI_ABIS as readonly string[]).includes(abi)) return abi as FfiAbi
  }
  return undefined
}

/** The path the native library must have inside an apk for `abi`. */
export function apkLibPath(abi: FfiAbi): string {
  return `lib/${abi}/libexponential_ui_ffi.so`
}

/** Does an `unzip -l <apk>` listing carry the FFI library for `abi`? */
export function apkListingHasFfi(listing: string, abi: FfiAbi): boolean {
  const wanted = apkLibPath(abi)
  return listing.split(`\n`).some((line) => line.trim().endsWith(wanted))
}

/* ------------------------------------------------------------- on-screen gate */

/**
 * The package owning the focused window, from `adb shell dumpsys window`
 * (`mCurrentFocus=Window{1a2b u0 <package>/<activity>}`). A crash or ANR
 * dialog (`Window{… u0 Application Error: <package>}`) names no activity and
 * is NOT the app, so it yields undefined; so does `mCurrentFocus=null`.
 */
export function focusedPackage(dumpsys: string): string | undefined {
  for (const line of dumpsys.split(`\n`)) {
    const match = /mCurrentFocus=Window\{\S+ \S+ ([^\s/{}]+)\/[^\s}]+\}/.exec(line)
    if (match) return match[1]
  }
  return undefined
}

/**
 * The example app's "first layout pass done" marker: `Bench.logCold` logs
 * `exponential-ui: surface=<id> phase=cold …` under the `ExponentialUI` tag
 * once the surface has laid out at least once (i.e. the `.so` loaded and the
 * specimen resolved). A missing specimen logs `specimen <id>: not in
 * specimens.json` and renders nothing worth storing.
 */
export function specimenState(
  logcat: string,
  viewId: string
): { ready: boolean; error?: string } {
  if (logcat.includes(`specimen ${viewId}: not in specimens.json`)) {
    return { ready: false, error: `the example app has no specimen \`${viewId}\` (fixtures/specimens.json)` }
  }
  return { ready: /exponential-ui: surface=\S* phase=cold\b/.test(logcat) }
}

export interface OnScreenProbe {
  /** `adb shell pidof <appId>` stdout (empty = not running). */
  pidof: string
  /** `adb shell dumpsys window` stdout. */
  dumpsys: string
}

/**
 * Is `appId` alive AND the focused window? The verdict a screenshot needs:
 * a dead app (the `dlopen` crash) or any other window on top (the launcher, a
 * crash dialog, a system prompt) means the frame is not the app's.
 */
export function appOnScreen(appId: string, probe: OnScreenProbe): { ok: true } | { ok: false; reason: string } {
  if (probe.pidof.trim() === ``) return { ok: false, reason: `${appId} is not running (crashed on start?)` }
  const focused = focusedPackage(probe.dumpsys)
  if (focused !== appId) {
    return { ok: false, reason: `${appId} is not in the foreground (focused: ${focused ?? `none`})` }
  }
  return { ok: true }
}
