/**
 * EXP-1264: the android `package` lane photographed the launcher behind a
 * crashed example app and stored it as 84 `updated` shots. These pin the pure
 * gates that now stand between a launch and the shutter, plus the SDK lookup
 * the example's gradle build needs.
 */
import { describe, expect, test } from "bun:test"
import {
  apkListingHasFfi,
  appOnScreen,
  deviceFfiAbi,
  focusedPackage,
  resolveAndroidSdk,
  sdkDirFromLocalProperties,
  specimenState,
} from "./android.ts"

const APP = `at.exponential.ui.kitchensink`

describe(`sdkDirFromLocalProperties`, () => {
  test(`reads sdk.dir, skipping comments`, () => {
    expect(sdkDirFromLocalProperties(`# generated\nsdk.dir=/Users/me/Library/Android/sdk\n`)).toBe(
      `/Users/me/Library/Android/sdk`
    )
  })
  test(`unescapes the Java-properties form Android Studio writes on Windows`, () => {
    expect(sdkDirFromLocalProperties(`sdk.dir=C\\:\\\\Users\\\\me\\\\sdk`)).toBe(`C:\\Users\\me\\sdk`)
  })
  test(`no sdk.dir = undefined`, () => {
    expect(sdkDirFromLocalProperties(`ndk.dir=/x\n#sdk.dir=/y`)).toBeUndefined()
  })
})

describe(`resolveAndroidSdk`, () => {
  const all = () => true
  test(`apps/android/local.properties wins over the env`, () => {
    expect(
      resolveAndroidSdk({ localProperties: `sdk.dir=/props`, env: { ANDROID_HOME: `/env` }, home: `/h`, exists: all })
    ).toEqual({ path: `/props`, source: `apps/android/local.properties` })
  })
  test(`then ANDROID_HOME, then ANDROID_SDK_ROOT`, () => {
    expect(resolveAndroidSdk({ env: { ANDROID_HOME: `/env` }, home: `/h`, exists: all })?.source).toBe(`ANDROID_HOME`)
    expect(resolveAndroidSdk({ env: { ANDROID_SDK_ROOT: `/root` }, home: `/h`, exists: all })?.path).toBe(`/root`)
  })
  test(`then ~/Library/Android/sdk`, () => {
    expect(resolveAndroidSdk({ env: {}, home: `/h`, exists: all })).toEqual({
      path: `/h/Library/Android/sdk`,
      source: `default`,
    })
  })
  test(`a path that does not exist is skipped`, () => {
    expect(
      resolveAndroidSdk({
        localProperties: `sdk.dir=/gone`,
        env: { ANDROID_HOME: `/env` },
        home: `/h`,
        exists: (path) => path === `/env`,
      })
    ).toEqual({ path: `/env`, source: `ANDROID_HOME` })
    expect(resolveAndroidSdk({ env: {}, home: `/h`, exists: () => false })).toBeUndefined()
  })
})

describe(`deviceFfiAbi`, () => {
  test(`an Apple-silicon emulator builds arm64`, () => {
    expect(deviceFfiAbi(`arm64-v8a\n`)).toBe(`arm64-v8a`)
  })
  test(`the device's first buildable preference wins`, () => {
    expect(deviceFfiAbi(`x86_64,arm64-v8a`)).toBe(`x86_64`)
    expect(deviceFfiAbi(`x86,armeabi-v7a,armeabi`)).toBe(`armeabi-v7a`)
  })
  test(`nothing buildable = undefined`, () => {
    expect(deviceFfiAbi(`x86`)).toBeUndefined()
    expect(deviceFfiAbi(``)).toBeUndefined()
  })
})

describe(`apkListingHasFfi`, () => {
  const listing = [
    `  Length      Date    Time    Name`,
    `---------  ---------- -----   ----`,
    `   123456  01-01-1981 01:01   lib/arm64-v8a/libexponential_ui_ffi.so`,
    `   654321  01-01-1981 01:01   lib/arm64-v8a/libjnidispatch.so`,
  ].join(`\n`)
  test(`finds the ABI's library`, () => {
    expect(apkListingHasFfi(listing, `arm64-v8a`)).toBe(true)
  })
  test(`an apk built without it (the crash on start) is refused`, () => {
    expect(apkListingHasFfi(listing, `x86_64`)).toBe(false)
    expect(apkListingHasFfi(listing.replace(`libexponential_ui_ffi`, `libother`), `arm64-v8a`)).toBe(false)
  })
})

describe(`focusedPackage`, () => {
  test(`the focused activity's package`, () => {
    expect(
      focusedPackage(`  mCurrentFocus=Window{1a2b3c u0 ${APP}/${APP}.MainActivity}\n  mFocusedApp=ActivityRecord{x}`)
    ).toBe(APP)
  })
  test(`the launcher is a different package`, () => {
    expect(
      focusedPackage(
        `  mCurrentFocus=Window{9f u0 com.google.android.apps.nexuslauncher/com.google.android.apps.nexuslauncher.NexusLauncherActivity}`
      )
    ).toBe(`com.google.android.apps.nexuslauncher`)
  })
  test(`a crash dialog or no focus is not the app`, () => {
    expect(focusedPackage(`  mCurrentFocus=Window{4d u0 Application Error: ${APP}}`)).toBeUndefined()
    expect(focusedPackage(`  mCurrentFocus=null`)).toBeUndefined()
  })
})

describe(`appOnScreen`, () => {
  const focused = `mCurrentFocus=Window{1 u0 ${APP}/.MainActivity}`
  test(`alive and focused = ok`, () => {
    expect(appOnScreen(APP, { pidof: `4242\n`, dumpsys: focused })).toEqual({ ok: true })
  })
  test(`a dead app is refused (the dlopen crash)`, () => {
    const verdict = appOnScreen(APP, { pidof: ``, dumpsys: focused })
    expect(verdict.ok).toBe(false)
  })
  test(`alive but behind the launcher is refused`, () => {
    const verdict = appOnScreen(APP, {
      pidof: `4242`,
      dumpsys: `mCurrentFocus=Window{2 u0 com.google.android.apps.nexuslauncher/.NexusLauncherActivity}`,
    })
    expect(verdict).toEqual({
      ok: false,
      reason: `${APP} is not in the foreground (focused: com.google.android.apps.nexuslauncher)`,
    })
  })
})

describe(`specimenState`, () => {
  test(`ready once the cold pass is logged`, () => {
    expect(
      specimenState(
        `I ExponentialUI: exponential-ui: surface=specimen phase=cold pass=1 layout_ns=1 wall_ns=2 upcalls=0 measure_calls=0 nodes=3`,
        `exponential-ui-kbd`
      )
    ).toEqual({ ready: true })
  })
  test(`not ready before it`, () => {
    expect(specimenState(``, `exponential-ui-kbd`)).toEqual({ ready: false })
    expect(specimenState(`I ExponentialUI: exponential-ui: surface=x phase=warm pass=1`, `v`).ready).toBe(false)
  })
  test(`an unknown specimen is an error, not a wait`, () => {
    const state = specimenState(`W ExponentialUI: specimen exponential-ui-nope: not in specimens.json`, `exponential-ui-nope`)
    expect(state.ready).toBe(false)
    expect(state.error).toContain(`exponential-ui-nope`)
  })
})
