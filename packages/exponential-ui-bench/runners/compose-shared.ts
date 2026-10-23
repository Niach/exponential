// Shared by the two Compose runners: the Android SDK location, the host
// build of the facade (the JVM tests load it through JNA) and `--repeat`.
import { existsSync, writeFileSync } from "node:fs"
import { homedir } from "node:os"
import { join } from "node:path"
import { spawnSync } from "node:child_process"
import { REPO_ROOT } from "../src/results"

/** `local.properties` (gitignored) pointing at the SDK; returns the SDK dir. */
export function androidSdk(compose: string): string {
  const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT || join(homedir(), `Library/Android/sdk`)
  const props = join(compose, `local.properties`)
  if (!existsSync(props)) writeFileSync(props, `sdk.dir=${sdk}\n`)
  return sdk
}

/** The release host build of `exponential-ui-ffi` (`--features cli`, what the Gradle test config loads); returns CARGO_TARGET_DIR. */
export function hostFacade(build = true): string {
  const target = process.env.CARGO_TARGET_DIR || join(REPO_ROOT, `apps/desktop/target`)
  if (build) {
    const r = spawnSync(`cargo`, [`build`, `-p`, `exponential-ui-ffi`, `--release`, `--features`, `cli`], { cwd: join(REPO_ROOT, `apps/desktop`), stdio: `inherit`, env: { ...process.env, CARGO_TARGET_DIR: target } })
    if (r.status !== 0) process.exit(r.status ?? 1)
  }
  return target
}

export function repeatArg(fallback = 3): number {
  const i = process.argv.indexOf(`--repeat`)
  return i >= 0 ? Number(process.argv[i + 1]) || fallback : fallback
}
