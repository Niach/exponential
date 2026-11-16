/**
 * EXP-1839: the iOS lanes built the pre-#PERF-11 libraries (the standalone
 * genUI and P2P xcframeworks) while the app links ONE mobile FFI, so a fresh
 * checkout failed `tuist generate` until someone built it by hand. These pin
 * when the lane builds it.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { IOS_MOBILE_FFI, iosFfiProblems, iosProjectMissing, iosSimArch, iosWorkspaceSplit, tuistPath } from "./ios.ts"

const ROOT = `/repo`
const [mobile, ui, engine] = IOS_MOBILE_FFI.frameworks.map((framework) => `${ROOT}/${framework.xcframework}`)

/** A file system holding `dirs` (path → entries) and the `files`. */
function fs(dirs: Record<string, string[]>, files: string[] = []) {
  return {
    root: ROOT,
    arch: `arm64`,
    exists: (path: string) => path in dirs || files.includes(path),
    list: (path: string) => dirs[path] ?? [],
  }
}

const built = () => ({
  dirs: {
    [mobile!]: [`Info.plist`, `ios-arm64-simulator`],
    [ui!]: [`Info.plist`, `ios-arm64-simulator`],
    [engine!]: [`Info.plist`, `ios-arm64-simulator`],
  } as Record<string, string[]>,
  files: [`${ui}/LINK_STUB`, `${engine}/LINK_STUB`],
})

describe(`iosFfiProblems`, () => {
  test(`a fresh checkout builds the mobile FFI`, () => {
    expect(iosFfiProblems(fs({}))).toEqual(IOS_MOBILE_FFI.frameworks.map((framework) => `${framework.xcframework} is missing`))
  })
  test(`the mobile FFI with its two link stubs and this Mac's slice is enough`, () => {
    const { dirs, files } = built()
    expect(iosFfiProblems(fs(dirs, files))).toEqual([])
  })
  test(`a device-only build (the release archive) has no simulator slice`, () => {
    const { dirs, files } = built()
    dirs[mobile!] = [`Info.plist`, `ios-arm64`]
    expect(iosFfiProblems(fs(dirs, files))).toEqual([`${IOS_MOBILE_FFI.frameworks[0].xcframework} has no arm64 simulator slice`])
  })
  test(`an Intel Mac needs the x86_64 slice`, () => {
    const { dirs, files } = built()
    expect(iosFfiProblems({ ...fs(dirs, files), arch: `x64` })).toHaveLength(3)
    for (const dir of Object.keys(dirs)) dirs[dir] = [`ios-arm64_x86_64-simulator`]
    expect(iosFfiProblems({ ...fs(dirs, files), arch: `x64` })).toEqual([])
  })
  test(`a facade's standalone build over its stub links the code twice: rebuild`, () => {
    const { dirs, files } = built()
    const problems = iosFfiProblems(fs(dirs, files.filter((file) => !file.startsWith(ui!))))
    expect(problems).toEqual([`${IOS_MOBILE_FFI.frameworks[1].xcframework} is the facade's standalone library, not the mobile FFI's link stub`])
  })
})

describe(`iosSimArch`, () => {
  test(`Apple silicon builds the arm64 simulator target`, () => {
    expect(iosSimArch(`arm64`)).toEqual({ rust: `aarch64-apple-ios-sim`, slice: `arm64` })
    expect(iosSimArch(`x64`)).toEqual({ rust: `x86_64-apple-ios`, slice: `x86_64` })
  })
})

describe(`iosProjectMissing`, () => {
  test(`a project generated before the FFI existed is refused`, () => {
    expect(iosProjectMissing(``)).toHaveLength(3)
    expect(iosProjectMissing(`ENET_NODE ExponentialUIWrapper ExponentialEngine`)).toEqual([])
  })
})

describe(`the paths the app links`, () => {
  const repo = join(import.meta.dir, `../../..`)
  test(`every framework is one apps/ios or the build script names`, () => {
    const project = readFileSync(join(repo, `apps/ios/Project.swift`), `utf8`)
    const script = readFileSync(join(repo, IOS_MOBILE_FFI.script), `utf8`)
    expect(project).toContain(IOS_MOBILE_FFI.frameworks[0].xcframework.replace(/^apps\//, `../`))
    expect(script).toContain(IOS_MOBILE_FFI.targetsEnv)
    expect(script).toContain(`LINK_STUB`)
    for (const framework of IOS_MOBILE_FFI.frameworks.slice(1)) {
      expect(script).toContain(framework.xcframework.replace(/^packages\//, `$REPO/packages/`).replace(/^apps\/desktop\/crates\//, `$CRATES/`))
    }
  })
})

/**
 * #EXP-1886: a checkout under /tmp (= /private/tmp) failed the build with
 * "Multiple commands produce" on the FFI xcframeworks: tuist 4.215 put the
 * app project at the real path and the local packages at the standardized one.
 */
describe(`a checkout behind macOS's /tmp link`, () => {
  const same = (a: string, b: string) => a.replace(/^\/private/, ``) === b.replace(/^\/private/, ``)
  test(`tuist gets the /private-less spelling, the one it gives the local packages`, () => {
    expect(tuistPath(`/private/tmp/feed256-master/apps/ios`, same)).toBe(`/tmp/feed256-master/apps/ios`)
    expect(tuistPath(`/private/var/folders/x/apps/ios`, same)).toBe(`/var/folders/x/apps/ios`)
    expect(tuistPath(`/Users/me/repo/apps/ios`, same)).toBe(`/Users/me/repo/apps/ios`)
    // A real /private/tmp that /tmp does not name stays as it is.
    expect(tuistPath(`/private/tmp/x`, () => false)).toBe(`/private/tmp/x`)
    expect(tuistPath(`/private/tmpfoo/x`, same)).toBe(`/private/tmpfoo/x`)
  })
  test(`a workspace that reaches the checkout by both spellings is caught`, () => {
    // The workspace `tuist generate` wrote from /private/tmp/c3-ios/apps/ios.
    const split = `<Workspace version = "1.0">
   <Group location = "group:../../../.." name = "private">
      <Group location = "group:tmp" name = "tmp"></Group>
   </Group>
   <Group location = "group:../../../../../tmp" name = "tmp">
      <FileRef location = "group:exponential-ui-swift/ExponentialUI.xcodeproj"></FileRef>
   </Group>
</Workspace>`
    expect(iosWorkspaceSplit(split)).toBe(true)
    // …and with `--path /tmp/c3-ios/apps/ios`.
    expect(iosWorkspaceSplit(`<Workspace><Group location = "group:../../packages" name = "packages"></Group></Workspace>`)).toBe(false)
  })
})
