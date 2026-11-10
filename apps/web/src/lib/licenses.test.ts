// EXP-375 — the drift and coverage gates for the third-party notices.
//
// Sibling of icons.test.ts, which locks the icon registry the same way: read
// repo files, re-run the generator, byte-compare, fail loudly. Deliberately
// compares CONTENTS rather than `git status`, so it behaves identically on a
// clean checkout, a dirty tree and in CI.
//
// This file runs in the pure-bun `web` job, which has no cargo, no gradle and
// no Android SDK. That is why the pipeline is split: the toolchain-dependent
// `collect:*` scripts write committed inventories, and everything here works
// off those inventories plus the lockfiles they were derived from. The coverage
// gates are what make a committed inventory trustworthy without re-running the
// collector — they prove, in both directions, that it still matches the
// lockfile it came from.

import { describe, expect, it } from "vitest"
import { execFileSync } from "node:child_process"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  parseBunLock,
  productionClosure,
} from "../../../../packages/licenses/src/bun-lock"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const pkg = join(repoRoot, `packages/licenses`)

const read = (path: string): string => readFileSync(join(repoRoot, path), `utf8`)
const readPkg = (path: string): string => readFileSync(join(pkg, path), `utf8`)

interface Component {
  name: string
  version: string
  declared: string
  licenses: string[]
  election?: string
  homepage?: string
  copyright?: string[]
  texts: { spdx: string; source: string; body: string }[]
  platforms?: string[]
  buildOnly?: true
}
interface Inventory {
  ecosystem: string
  scope: string
  components: Component[]
  roots?: string[]
  triples?: string[]
}

const inventory = (file: string): Inventory =>
  JSON.parse(readPkg(`inventory/${file}`)) as Inventory

const rust = inventory(`rust.json`)
const npmWeb = inventory(`npm-web.json`)
const npmSite = inventory(`npm-site.json`)
const ios = inventory(`ios.json`)
const android = inventory(`android.json`)
// #VAPP-270: the runner kits.
const rustRunner = {
  desktop: inventory(`rust-runner-desktop.json`),
  android: inventory(`rust-runner-android.json`),
  ios: inventory(`rust-runner-ios.json`),
  web: inventory(`rust-runner-web.json`),
}
const npmRunnerWeb = inventory(`npm-runner-web.json`)
const gradleRunnerAndroid = inventory(`gradle-runner-android.json`)

/** Where each client's notice is committed. Mirrors OUTPUTS in generate.ts. */
const OUTPUTS = {
  desktop: `apps/desktop/assets/licenses/NOTICES.txt`,
  web: `apps/web/public/NOTICES.txt`,
  site: `apps/site/public/NOTICES.txt`,
  ios: `apps/ios/Exponential/Resources/NOTICES.txt`,
  android: `apps/android/app/src/main/assets/NOTICES.txt`,
  // #VAPP-270: one per runner kit; scripts/runner-kits/<target>.sh copies it
  // into the kit and `exponential bundle` ships it in every export.
  [`runner-desktop`]: `apps/desktop/crates/eengine-runner/NOTICES.txt`,
  [`runner-android`]: `apps/runner/android/NOTICES.txt`,
  [`runner-ios`]: `apps/runner/ios/NOTICES.txt`,
  [`runner-web`]: `apps/runner/web/public/NOTICES.txt`,
} as const
const RUNNER_CLIENTS = [`runner-desktop`, `runner-android`, `runner-ios`, `runner-web`] as const

const notices = Object.fromEntries(
  Object.entries(OUTPUTS).map(([client, path]) => [client, read(path)])
) as Record<keyof typeof OUTPUTS, string>

// ---------------------------------------------------------------------------

describe(`notice generation`, () => {
  it(`committed NOTICES.txt files are current`, () => {
    const before = new Map(
      Object.values(OUTPUTS).map((p) => [p, read(p)] as const)
    )
    execFileSync(`bun`, [`run`, `scripts/generate.ts`], {
      cwd: pkg,
      stdio: `pipe`,
    })
    for (const [path, contents] of before) {
      expect(
        read(path),
        `${path} is stale — run \`bun run --filter @exp/licenses generate\``
      ).toBe(contents)
    }
  })

  it(`every client has a non-empty, well-formed notice`, () => {
    for (const [client, contents] of Object.entries(notices)) {
      expect(contents.length, client).toBeGreaterThan(1000)
      expect(contents, `${client} must not contain CRLF`).not.toMatch(/\r/)
      expect(contents, `${client} must end in exactly one newline`).toMatch(
        /[^\n]\n$/
      )
      expect(
        /[ \t]+\n/.test(contents),
        `${client} has trailing whitespace`
      ).toBe(false)
      expect(contents, client).toContain(`EXPONENTIAL — THIRD-PARTY NOTICES`)
    }
  })

  it(`headings are 78-column rule sandwiches — the mobile splitters' contract`, () => {
    // EXP-262: iOS (ExpCore/Sources/ThirdPartyNotices.swift) and Android
    // (domain/ThirdPartyNotices.kt) split the notice into sections on exactly
    // this shape: a 78-char `=`/`-` rule, one or two title lines, a matching
    // closing rule. If render.ts's WIDTH ever changes, this fails loudly
    // instead of the mobile licence screens quietly collapsing into one blob.
    const rule = (char: string) => char.repeat(78)
    for (const [client, contents] of Object.entries(notices)) {
      const lines = contents.split(`\n`)
      // The file header: rule, product title, client title, closing rule.
      expect(lines[0], client).toBe(rule(`=`))
      expect(lines[3], client).toBe(rule(`=`))
      // At least one licence-group `-` sandwich (rule, SPDX id, rule).
      const hasGroup = lines.some(
        (line, i) =>
          line === rule(`-`) &&
          lines[i + 1]?.trim() &&
          lines[i + 2] === rule(`-`)
      )
      expect(hasGroup, `${client} has no licence-group heading`).toBe(true)
    }
  })

  it(`notices carry nothing host-specific`, () => {
    // A generated file that embeds a path, a hostname or a build date can never
    // be byte-stable across two machines, and the drift gate above would then
    // fail forever on someone else's laptop.
    for (const [client, contents] of Object.entries(notices)) {
      expect(contents, `${client} leaks an absolute path`).not.toMatch(
        /\/(Users|home)\/[a-z]/i
      )
      expect(contents, `${client} leaks a target dir`).not.toContain(
        `node_modules/`
      )
      expect(contents, `${client} looks like it embeds a build date`).not.toMatch(
        /\bGenerated (on|at)\b/i
      )
    }
  })

  it(`the pipeline never sorts with the host's locale`, () => {
    // localeCompare depends on the runner's ICU build, so macOS and ubuntu
    // would disagree on ordering and the drift gate would flap.
    for (const file of [
      `src/schema.ts`,
      `src/bun-lock.ts`,
      `src/spdx.ts`,
      `src/text.ts`,
      `src/render.ts`,
      `scripts/generate.ts`,
      `scripts/collect-rust.ts`,
      `scripts/collect-npm.ts`,
      `scripts/collect-ios.ts`,
      `scripts/collect-android.ts`,
    ]) {
      // `.localeCompare(` — a call, not the comments that explain why not to.
      expect(readPkg(file), `${file} calls localeCompare`).not.toMatch(
        /\.localeCompare\(/
      )
    }
  })
})

// ---------------------------------------------------------------------------
// Coverage — every package in every lockfile is accounted for, both ways
// ---------------------------------------------------------------------------

describe(`coverage — Rust`, () => {
  const excluded = JSON.parse(readPkg(`inventory/rust-excluded.json`)) as {
    excluded: { package: string; reason: string }[]
  }

  const lock = new Set<string>()
  for (const block of read(`apps/desktop/Cargo.lock`)
    .split(/\n\[\[package\]\]\n/)
    .slice(1)) {
    const name = block.match(/^name = "([^"]+)"/m)?.[1]
    const version = block.match(/^version = "([^"]+)"/m)?.[1]
    if (name && version) lock.add(`${name}@${version}`)
  }

  const attributed = new Set(rust.components.map((c) => `${c.name}@${c.version}`))
  const dropped = new Set(excluded.excluded.map((e) => e.package))

  it(`every Cargo.lock package is attributed or excluded with a reason`, () => {
    const uncovered = [...lock].filter(
      (p) => !attributed.has(p) && !dropped.has(p)
    )
    expect(
      uncovered,
      `uncovered crates — run \`bun run --filter @exp/licenses collect:rust\``
    ).toEqual([])
    for (const entry of excluded.excluded) {
      expect(entry.reason.length, entry.package).toBeGreaterThan(20)
    }
  })

  it(`no attributed crate has left Cargo.lock`, () => {
    // The other direction: a removed dependency must not leave a stale notice.
    const stale = [...attributed].filter((p) => !lock.has(p))
    expect(stale, `stale entries in inventory/rust.json`).toEqual([])
    const staleExclusions = [...dropped].filter((p) => !lock.has(p))
    expect(staleExclusions, `stale entries in rust-excluded.json`).toEqual([])
  })

  it(`attributed and excluded are disjoint`, () => {
    const both = [...attributed].filter((p) => dropped.has(p))
    expect(both).toEqual([])
  })

  it(`Cargo.lock is not mistaken for the shipped graph`, () => {
    // The canary from the issue: libfuzzer-sys is `(MIT OR Apache-2.0) AND
    // NCSA`, NCSA is not on deny.toml's allow-list, and it reaches the lockfile
    // only as rav1e's optional fuzzing dependency. If it is ever attributed,
    // the collector has stopped filtering the graph.
    expect([...attributed].filter((p) => p.startsWith(`libfuzzer-sys@`))).toEqual(
      []
    )
    expect(lock.size).toBeGreaterThan(attributed.size)
  })
})

describe(`coverage — npm`, () => {
  // The committed inventory needs node_modules to COLLECT (licence bodies,
  // copyright lines), but the SET of components it must contain is a pure
  // function of bun.lock: a workspace's `dependencies` + `optionalDependencies`
  // walked transitively, our `@exp/*` packages traversed through. So the gate
  // recomputes that closure here, with no toolchain, and demands equality in
  // both directions — a new transitive package fails it, a removed one does.
  //
  // EXP-1205: the previous gate only checked the two apps' DIRECT
  // dependencies. EXP-887 then added the shadcn set to `@exp/ui`, which both
  // apps reach through a workspace edge, and the inventories went 1 (web) and
  // 128 (marketing) components short for weeks while the gate stayed green.
  const lock = parseBunLock(read(`bun.lock`))

  const cases = [
    { scope: `apps/web`, inv: npmWeb },
    { scope: `apps/site`, inv: npmSite },
    { scope: `apps/runner/web`, inv: npmRunnerWeb },
  ]

  it.each(cases)(`$scope: the inventory IS bun.lock's production closure`, ({ scope, inv }) => {
    const closure = productionClosure(lock, scope).map(
      (c) => `${c.name}@${c.version}`
    )
    expect(closure.length, `${scope} resolves no dependencies?`).toBeGreaterThan(0)
    const attributed = inv.components.map((c) => `${c.name}@${c.version}`)
    const missing = closure.filter((id) => !attributed.includes(id))
    const stale = attributed.filter((id) => !closure.includes(id))
    expect(
      missing,
      `${scope}: in bun.lock's production closure but not attributed — run ` +
        `\`bun run --filter @exp/licenses collect:npm && bun run notices\``
    ).toEqual([])
    expect(
      stale,
      `${scope}: attributed but no longer in bun.lock's production closure — run ` +
        `\`bun run --filter @exp/licenses collect:npm && bun run notices\``
    ).toEqual([])
  })

  it.each(cases)(`$scope: no workspace package is attributed`, ({ inv }) => {
    // Our own code is covered by the repository LICENSE, not by a notice entry.
    expect(inv.components.filter((c) => c.name.startsWith(`@exp/`))).toEqual([])
  })

  it(`the shadcn CLI stays out of every production closure`, () => {
    // EXP-1205: `shadcn` is the generator behind `bunx shadcn add`; nothing
    // imports it. Declared under `dependencies` of `@exp/ui` it dragged ~200
    // packages (msw, ts-morph, inquirer, ora, …) into both closures.
    for (const inv of [npmWeb, npmSite, npmRunnerWeb]) {
      expect(inv.components.map((c) => c.name)).not.toContain(`shadcn`)
    }
    const ui = lock.workspaces.get(`packages/ui`)!
    expect(ui.dependencies).not.toHaveProperty(`shadcn`)
  })

  it(`platform-gated packages are attributed on every host`, () => {
    // ~55 packages in the closure are os/cpu-gated (@esbuild/*, @rollup/*,
    // lightningcss-*). Only the current host's copy is
    // ever installed, so a collector that read them off disk would emit a
    // different file on macOS and on ubuntu, and the drift gate would fail
    // forever. Assert the full family is present, not just this host's member.
    const names = new Set(npmWeb.components.map((c) => c.name))
    for (const family of [`lightningcss-darwin-arm64`, `lightningcss-linux-x64-gnu`]) {
      expect(names, `${family} missing — host-dependent collection?`).toContain(
        family
      )
    }
  })
})

describe(`coverage — iOS`, () => {
  const resolved = JSON.parse(read(`apps/ios/Tuist/Package.resolved`)) as {
    pins: { identity: string; state: { version?: string } }[]
  }

  it(`every SwiftPM pin is attributed, and nothing else is`, () => {
    const pins = resolved.pins.map((p) => p.identity).sort()
    const attributed = ios.components.map((c) => c.name).sort()
    expect(attributed).toEqual(pins)
  })

  it(`pinned versions match`, () => {
    const versions = new Map(
      resolved.pins.map((p) => [p.identity, p.state.version])
    )
    for (const component of ios.components) {
      expect(component.version, component.name).toBe(versions.get(component.name))
    }
  })
})

describe(`coverage — Android`, () => {
  const toml = read(`apps/android/gradle/libs.versions.toml`)
  const librariesBlock = toml.slice(
    toml.indexOf(`[libraries]`),
    toml.indexOf(`[plugins]`)
  )
  /** `group:name` for every alias the version catalogue declares. */
  const declared = [...librariesBlock.matchAll(/group\s*=\s*"([^"]+)"[^\n]*name\s*=\s*"([^"]+)"/g)].map(
    (m) => `${m[1]}:${m[2]}`
  )
  const attributed = new Set(android.components.map((c) => c.name))

  // The catalogue also declares artifacts that never reach the release APK:
  // test and androidTest dependencies, `debugImplementation` tooling, and KSP
  // annotation processors that run at compile time. The collector resolves the
  // productionRelease runtime configuration, so none of these appear — and
  // none of them may be attributed either.
  const NOT_IN_RELEASE_APK = [
    // testImplementation / androidTestImplementation
    `junit:junit`,
    `androidx.test.ext:junit`,
    `androidx.test:rules`,
    `androidx.test.espresso:espresso-core`,
    `tools.fastlane:screengrab`,
    `androidx.compose.ui:ui-test-junit4`,
    `io.ktor:ktor-client-mock`,
    // debugImplementation — apps/android/app/build.gradle.kts:196,199
    `androidx.compose.ui:ui-tooling`,
    `androidx.compose.ui:ui-test-manifest`,
    // ksp(...) — compile-time processors, build.gradle.kts:162,179
    `com.google.dagger:hilt-compiler`,
    `androidx.room:room-compiler`,
    // EXP-1280: `:ui-compose` (the Exponential UI painter the app links) resolves
    // its testImplementation screenshot stack through this catalogue.
    `org.robolectric:robolectric`,
    `io.github.takahirom.roborazzi:roborazzi`,
    `io.github.takahirom.roborazzi:roborazzi-compose`,
    `io.github.takahirom.roborazzi:roborazzi-junit-rule`,
    // EXP-1521/1522: the `:macrobenchmark` test module (never in the app).
    `androidx.benchmark:benchmark-macro-junit4`,
    `androidx.test.uiautomator:uiautomator`,
  ]

  it(`every runtime library in the version catalogue is attributed`, () => {
    const missing = declared.filter(
      (a) => !NOT_IN_RELEASE_APK.includes(a) && !attributed.has(a)
    )
    expect(
      missing,
      `run \`bun run --filter @exp/licenses collect:android\``
    ).toEqual([])
  })

  it(`the BOM closure is attributed, not just the catalogue`, () => {
    // compose-bom and firebase-bom expand to artifacts libs.versions.toml never
    // names. If the inventory were merely the catalogue, this is what would be
    // missing — which is the whole reason the collector resolves the real
    // runtime configuration instead of parsing the TOML.
    expect(android.components.length).toBeGreaterThan(declared.length * 2)
    expect(attributed).toContain(`androidx.compose.runtime:runtime`)
  })

  it(`artifacts that never reach the release APK are not attributed`, () => {
    const leaked = NOT_IN_RELEASE_APK.filter((a) => attributed.has(a))
    expect(
      leaked,
      `test/debug/compile-time dependencies leaked into the shipped inventory`
    ).toEqual([])
  })
})

// ---------------------------------------------------------------------------
// The runner kits (#VAPP-270)
// ---------------------------------------------------------------------------
//
// Each kit's notice is the closure of what THAT runner ships, not of the app
// it shares a workspace with: `cargo tree -p <root> --target <triple>` per
// runner (collect-rust.ts), the web runner's bun.lock production closure
// (collect-npm.ts, gated above), and the Android runner's resolved `release`
// runtime graph (collect-android.ts --runner, through
// packages/licenses/gradle/runner-license.init.gradle.kts).

describe(`coverage — the runner kits`, () => {
  const lock = new Set<string>()
  for (const block of read(`apps/desktop/Cargo.lock`)
    .split(/\n\[\[package\]\]\n/)
    .slice(1)) {
    const name = block.match(/^name = "([^"]+)"/m)?.[1]
    const version = block.match(/^version = "([^"]+)"/m)?.[1]
    if (name && version) lock.add(`${name}@${version}`)
  }
  const workspace = new Set(
    (
      JSON.parse(readPkg(`inventory/rust-excluded.json`)) as {
        excluded: { package: string; reason: string }[]
      }
    ).excluded
      .filter((e) => e.reason.startsWith(`workspace member`))
      .map((e) => e.package.replace(/@.*$/, ``))
  )
  const keys = (inv: Inventory) => inv.components.map((c) => `${c.name}@${c.version}`)

  it.each(Object.entries(rustRunner))(`%s: the Rust closure is rooted at workspace crates, over its triples`, (_, inv) => {
    expect(inv.roots?.length, `${inv.scope} has no roots`).toBeGreaterThan(0)
    for (const root of inv.roots!) expect(workspace, `${root} is not a workspace crate`).toContain(root)
    expect(inv.triples?.length, `${inv.scope} has no triples`).toBeGreaterThan(0)
    expect(inv.components.length, `${inv.scope} resolves no crates?`).toBeGreaterThan(10)
    for (const c of inv.components) {
      for (const p of c.platforms ?? []) expect(inv.triples, `${c.name}: ${p}`).toContain(p)
    }
  })

  it.each(Object.entries(rustRunner))(`%s: every crate is in Cargo.lock, none is ours`, (_, inv) => {
    const stale = keys(inv).filter((k) => !lock.has(k))
    expect(stale, `stale crates in inventory/rust-${inv.scope}.json — run collect:rust`).toEqual([])
    expect(inv.components.filter((c) => workspace.has(c.name)).map((c) => c.name)).toEqual([])
  })

  it(`the desktop runner is a subset of the desktop graph`, () => {
    // Same triples, so what exponential-runner links must be in the
    // all-features workspace graph cargo-deny audits.
    const all = new Set(keys(rust))
    expect(rustRunner.desktop.triples).toEqual(
      [...new Set(rust.components.flatMap((c) => c.platforms ?? []))].sort()
    )
    expect(keys(rustRunner.desktop).filter((k) => !all.has(k))).toEqual([])
    // …and far less than it: the runner is not the IDE.
    expect(rustRunner.desktop.components.length).toBeLessThan(rust.components.length)
  })

  it(`the mobile runners link the Engine and genUI facades only`, () => {
    for (const inv of [rustRunner.android, rustRunner.ios]) {
      expect(inv.roots).toEqual([`eengine-ffi`, `exponential-ui-ffi`])
      // No hub (tokio/hyper/rmcp): the Engine's default features leave it out.
      for (const crate of [`tokio`, `hyper`, `rmcp`, `gpui`]) {
        expect(inv.components.map((c) => c.name), `${inv.scope} ships ${crate}`).not.toContain(crate)
      }
      expect(inv.components.map((c) => c.name)).toContain(`uniffi_core`)
    }
    expect(rustRunner.android.triples).toEqual([`aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`])
    expect(rustRunner.ios.triples).toEqual([`aarch64-apple-ios`])
  })

  it(`the web runner's one Rust artifact is genUI Motion's wasm`, () => {
    expect(rustRunner.web.roots).toEqual([`eui-motion-wasm`])
    expect(rustRunner.web.triples).toEqual([`wasm32-unknown-unknown`])
    expect(rustRunner.web.components.map((c) => c.name)).toContain(`wasm-bindgen`)
  })

  it(`the iOS runner has no Swift package dependency of its own`, () => {
    // Its notice is the two Rust closures: RunnerKit, the Engine's and
    // genUI's Swift packages resolve nothing remote. A `.package(url:` here
    // needs an inventory.
    for (const path of [
      `apps/runner/ios/Package.swift`,
      `apps/desktop/crates/eengine-ffi/Package.swift`,
      `packages/exponential-ui-swift/Package.swift`,
    ]) {
      expect(read(path), path).not.toMatch(/\.package\(\s*url:/)
    }
  })

  describe(`the Android runner's Gradle closure`, () => {
    const catalog = read(`packages/exponential-ui-compose/gradle/libs.versions.toml`)
    const libraries = catalog.slice(catalog.indexOf(`[libraries]`), catalog.indexOf(`[plugins]`))
    const alias = new Map(
      [...libraries.matchAll(/^([\w-]+)\s*=\s*\{\s*group\s*=\s*"([^"]+)",\s*name\s*=\s*"([^"]+)"/gm)].map(
        (m) => [m[1], `${m[2]}:${m[3]}`] as const
      )
    )
    /** `group:name` of every catalogue alias a build file names under the given configurations. */
    const named = (configurations: string[]): string[] => {
      const out = new Set<string>()
      for (const path of [
        `apps/runner/android/app/build.gradle.kts`,
        `apps/desktop/crates/eengine-ffi/android/build.gradle.kts`,
        `packages/exponential-ui-compose/ui-compose/build.gradle.kts`,
      ]) {
        for (const m of read(path).matchAll(/^\s*(\w+)\((.*)\)\s*$/gm)) {
          if (!configurations.includes(m[1]) || m[2].includes(`platform(`)) continue
          const ref = m[2].match(/libs\.([\w.]+?)(?:\.get\(\))?(?:\}|\)|$)/)?.[1]
          if (!ref || ref.startsWith(`versions.`)) continue
          const coordinate = alias.get(ref.replace(/\./g, `-`))
          expect(coordinate, `${path}: libs.${ref} is not in the catalogue`).toBeDefined()
          out.add(coordinate!)
        }
      }
      return [...out].sort()
    }
    const attributed = new Set(gradleRunnerAndroid.components.map((c) => c.name))

    it(`every runtime library the runner, the Engine AAR and eui-compose declare is attributed`, () => {
      const runtime = named([`implementation`, `api`])
      expect(runtime.length).toBeGreaterThan(5)
      expect(runtime.filter((a) => !attributed.has(a)), `run collect:android -- --runner`).toEqual([])
      // The BOM's expansion, not just the catalogue.
      expect(attributed).toContain(`androidx.compose.runtime:runtime`)
    })

    it(`test and debug artifacts never reach the runner's inventory`, () => {
      const notShipped = named([`testImplementation`, `androidTestImplementation`, `debugImplementation`]).filter(
        (a) => !named([`implementation`, `api`]).includes(a)
      )
      expect(notShipped.length).toBeGreaterThan(0)
      expect(notShipped.filter((a) => attributed.has(a))).toEqual([])
    })

    it(`JNA is attributed under Apache-2.0 alone`, () => {
      const notice = notices[`runner-android`]
      expect(notice).toContain(`net.java.dev.jna:jna`)
      expect(notice).not.toMatch(/^LGPL/m)
    })
  })
})

describe(`runner notices`, () => {
  it(`every export says whose app it is and reproduces Apache-2.0`, () => {
    for (const client of RUNNER_CLIENTS) {
      const contents = notices[client]
      expect(flat(contents), client).toContain(`This app was exported with Exponential`)
      expect(flat(contents), client).toContain(`is its author's and is not covered by this file`)
      expect(contents, `${client} reproduces no Apache-2.0 body`).toMatch(/^\s*Apache License\s*$/m)
    }
  })

  it(`every runner ships genUI's vendored and ported sources`, () => {
    for (const client of RUNNER_CLIENTS) {
      const vendored = section(notices[client], `Vendored source`)
      expect(vendored, `${client} has no vendored-source section`).toBeDefined()
      for (const title of [`A2UI specification schemas`, `json-render`, `QR Code generator`, `lottie-web`]) {
        expect(vendored, `${client}: ${title}`).toContain(title)
      }
    }
  })

  it(`the desktop runner embeds Inter and records gpui's patched-out GPL crates`, () => {
    const fonts = section(notices[`runner-desktop`], `Bundled fonts`)
    expect(fonts).toContain(`SIL OPEN FONT LICENSE Version 1.1`)
    expect(fonts).toContain(`Inter`)
    // assets.rs embeds `fonts/Inter-*.ttf` only.
    expect(fonts).not.toContain(`JetBrains Mono`)
    expect(read(`apps/desktop/crates/eengine-runner/src/assets.rs`)).toContain(`#[include = "fonts/Inter-*.ttf"]`)
    expect(section(notices[`runner-desktop`], `Components deliberately not included`)).toContain(`zlog`)
  })

  it(`the web runner carries SQLite's public-domain note, the Apache-2.0 glue and Emscripten`, () => {
    const web = notices[`runner-web`]
    const oss = section(web, `Open-source components`)!
    for (const pkg of [`@sqlite.org/sqlite-wasm`, `@noble/hashes`, `lucide-react`, `react`, `react-dom`]) {
      expect(oss, pkg).toContain(`  ${pkg} `)
    }
    const vendored = flat(section(web, `Vendored source`)!)
    expect(vendored).toContain(`dedicated to the public domain`)
    expect(vendored).toContain(`Emscripten SDK 5.0.5`)
    expect(vendored).toContain(`University of Illinois/NCSA Open Source License`)
    // The test-only IndexedDB shim never ships.
    expect(oss).not.toContain(`fake-indexeddb`)
    // The bundled sqlite-wasm is the version the notice names.
    expect(npmRunnerWeb.components.find((c) => c.name === `@sqlite.org/sqlite-wasm`)?.version).toBe(`3.54.1-build1`)
  })

  it(`the web runner's notice ships beside index.html`, () => {
    // vite copies public/ to the export's root.
    expect(OUTPUTS[`runner-web`].startsWith(`apps/runner/web/public/`)).toBe(true)
  })
})

// ---------------------------------------------------------------------------
// Licence election policy
// ---------------------------------------------------------------------------

describe(`licence election`, () => {
  const find = (inv: Inventory, name: string): Component => {
    const c = inv.components.find((x) => x.name === name)
    expect(c, `${name} is missing from inventory/${inv.scope}`).toBeDefined()
    return c!
  }

  it(`an OR elects exactly one licence`, () => {
    // The load-bearing case. self_cell is `Apache-2.0 OR GPL-2.0-only`; a
    // generator that concatenated both would assert GPL terms over our binary.
    const selfCell = find(rust, `self_cell`)
    expect(selfCell.declared).toContain(`GPL-2.0-only`)
    expect(selfCell.licenses).toEqual([`Apache-2.0`])
    expect(selfCell.election).toContain(`Apache-2.0 elected`)
  })

  it(`an AND keeps every conjunct`, () => {
    // unicode-ident is `(MIT OR Apache-2.0) AND Unicode-3.0` — the Unicode
    // terms are mandatory, not electable.
    const ident = find(rust, `unicode-ident`)
    expect(ident.licenses).toEqual([`MIT`, `Unicode-3.0`])
    expect(ident.texts.map((t) => t.spdx).sort()).toEqual([`MIT`, `Unicode-3.0`])
  })

  it(`no copyleft licence is ever elected`, () => {
    const elected = new Set(
      // (The Gradle inventories record JNA's raw `LGPL OR Apache` POM pair;
      // curated/overrides.ts elects Apache-2.0 at merge time, gated below.)
      [rust, ...Object.values(rustRunner), npmRunnerWeb].flatMap((inv) =>
        inv.components.flatMap((c) => c.licenses)
      )
    )
    for (const id of elected) {
      expect(id, `copyleft licence elected`).not.toMatch(/GPL|AGPL|LGPL|SSPL/)
    }
  })

  it(`every elected Rust licence is on deny.toml's allow-list`, () => {
    // Cross-gate: cargo-deny gates what may enter the graph; this gates what we
    // then claim in the notice. They must agree, or one of them is lying.
    const deny = read(`apps/desktop/deny.toml`)
    const allowBlock = deny.slice(deny.indexOf(`allow = [`))
    const allowed = new Set(
      [...allowBlock.slice(0, allowBlock.indexOf(`]`)).matchAll(/"([^"]+)"/g)].map(
        (m) => m[1]
      )
    )
    expect(allowed.size).toBeGreaterThan(10)
    const elected = new Set(
      [rust, ...Object.values(rustRunner)].flatMap((inv) =>
        inv.components.flatMap((c) => c.licenses)
      )
    )
    const notAllowed = [...elected].filter((id) => !allowed.has(id))
    expect(
      notAllowed,
      `elected but not on deny.toml's allow-list`
    ).toEqual([])
  })
})

// ---------------------------------------------------------------------------
// Section rules
// ---------------------------------------------------------------------------

/**
 * Everything between one numbered section heading and the next.
 *
 * Sections are delimited by their full heading BLOCK — a 78-column `=` rule,
 * the heading, another rule. Searching for a bare `^\d+\. ` instead would
 * split on "1. Definitions", which appears at column 0 inside both the MPL-2.0
 * and Apache-2.0 bodies, silently truncating every section that reproduces one.
 */
const RULE = `=`.repeat(78)
function sections(contents: string): Map<string, string> {
  const parts = contents.split(new RegExp(`\n${RULE}\n(\\d+\\. [^\n]+)\n${RULE}\n`))
  const out = new Map<string, string>()
  // parts[0] is the preamble; then [heading, body] pairs.
  for (let i = 1; i < parts.length; i += 2) {
    out.set(parts[i].replace(/^\d+\. /, ``), parts[i + 1] ?? ``)
  }
  return out
}
const SECTIONS = new Map(
  Object.entries(notices).map(([client, text]) => [client, sections(text)])
)
/** Collapse wrapping so a prose assertion is not defeated by a line break. */
const flat = (text: string): string => text.replace(/\s+/g, ` `)

function section(contents: string, title: string): string | undefined {
  for (const [client, text] of Object.entries(notices)) {
    if (text !== contents) continue
    for (const [heading, body] of SECTIONS.get(client)!) {
      if (new RegExp(`^${title}$`).test(heading)) return body
    }
    return undefined
  }
  throw new Error(`section() called with text that is not a committed notice`)
}

describe(`notice sections`, () => {
  it(`MPL-2.0 components carry a source-availability statement`, () => {
    const mpl = section(
      notices.desktop,
      `Mozilla Public License 2\\.0 — source availability`
    )
    expect(mpl, `desktop notice has no MPL section`).toBeDefined()
    expect(mpl).toContain(`cssparser`)
    expect(mpl).toContain(`Source Code Form:`)
    expect(mpl).toContain(`Distributed unmodified.`)
    // Attribution alone does not satisfy MPL §3.2 — the URL is the obligation.
    expect(mpl).toContain(`https://github.com/servo/rust-cssparser`)
  })

  it(`a build-only MPL crate is not claimed as a source obligation`, () => {
    // cbindgen is MPL-2.0 but runs during the build and is never linked, so
    // §3.2 has no binary recipient to inform.
    const cbindgen = rust.components.find((c) => c.name === `cbindgen`)
    expect(cbindgen?.buildOnly).toBe(true)
    const mpl = section(
      notices.desktop,
      `Mozilla Public License 2\\.0 — source availability`
    )
    expect(mpl).not.toContain(`cbindgen`)
  })

  it(`trademarks are in their own section, never under a licence heading`, () => {
    for (const client of [`desktop`, `web`, `site`, `android`, `ios`] as const) {
      const trademarks = section(notices[client], `Trademarks`)
      if (!trademarks) continue
      expect(flat(trademarks), client).toContain(
        `not affiliated with, endorsed by, or sponsored by`
      )
      const oss = section(notices[client], `Open-source components`)
      expect(oss, client).toBeDefined()
      // The marks must not be listed as licensed components.
      for (const mark of [`Sign in with Apple`, `Sign in with Google`]) {
        expect(oss, `${client}: trademark leaked into the OSS aggregate`).not.toContain(
          mark
        )
      }
    }
  })

  it(`desktop names the marks it actually ships`, () => {
    const trademarks = section(notices.desktop, `Trademarks`)!
    for (const mark of [`Apple logo`, `Google "G" logo`, `Claude logo`, `Codex logo`]) {
      expect(trademarks).toContain(mark)
    }
  })

  it(`non-OSS components are separated from the open-source aggregate`, () => {
    // docs/third-party-licences.md: iOS and Android carry Google's closed
    // binaries, and web, site and desktop carry nothing — the section must
    // be ABSENT rather than empty.
    for (const client of [`ios`, `android`] as const) {
      const commercial = section(notices[client], `Commercially licensed components`)
      expect(commercial, `${client} should have a commercial section`).toBeDefined()
      expect(flat(commercial!)).toContain(`NOT open source`)
    }
    for (const client of [`desktop`, `web`, `site`, ...RUNNER_CLIENTS] as const) {
      // docs/third-party-licences.md: "the section should be absent rather
      // than empty". The two web packages that declare no licence at all
      // (@better-fetch/fetch, creem) are both MIT — see curated/overrides.ts.
      expect(
        section(notices[client], `Commercially licensed components`),
        `${client} should have no commercial section`
      ).toBeUndefined()
    }
  })

  it(`iOS and Android route Google's closed binaries out of the aggregate`, () => {
    for (const [client, inv, marker] of [
      [`ios`, ios, `googleappmeasurement`],
      [`android`, android, `com.google.android.play:app-update-ktx`],
    ] as const) {
      const vendor = inv.components.filter((c) => c.licenses.length === 0)
      expect(vendor.length, `${client} vendor-terms components`).toBeGreaterThan(0)
      expect(vendor.map((c) => c.name)).toContain(marker)
      const oss = section(notices[client], `Open-source components`)!
      expect(oss, `${client}: vendor binary leaked into the OSS aggregate`).not.toContain(
        marker
      )
      const commercial = section(notices[client], `Commercially licensed components`)!
      expect(commercial).toContain(marker)
    }
  })

  it(`fonts reproduce the OFL wherever font binaries ship`, () => {
    for (const client of [`desktop`, `site`, `web`, `ios`, `android`] as const) {
      const fonts = section(notices[client], `Bundled fonts`)
      expect(fonts, `${client} ships font binaries`).toBeDefined()
      expect(fonts).toContain(`SIL OPEN FONT LICENSE Version 1.1`)
    }
    // #EXP-1629: the apps all ship JetBrains Mono (the theme's mono family).
    // The web app links Inter from Google Fonts at runtime and distributes no
    // Inter binary, so it must not claim Inter's OFL.
    for (const client of [`desktop`, `web`, `ios`, `android`] as const) {
      expect(section(notices[client], `Bundled fonts`), client).toContain(`JetBrains Mono`)
    }
    expect(section(notices.web, `Bundled fonts`)).not.toContain(`The Inter Project Authors`)
  })

  it(`every client attributes Lucide`, () => {
    for (const client of Object.keys(OUTPUTS) as (keyof typeof OUTPUTS)[]) {
      const icons = section(notices[client], `Bundled icons`)
      expect(icons, `${client} ships Lucide geometry`).toBeDefined()
      expect(icons).toContain(`ISC License`)
      expect(icons).toContain(`Lucide Contributors 2022`)
    }
  })

  it(`the desktop attributes its vendored crates`, () => {
    const vendored = section(notices.desktop, `Vendored source`)
    expect(vendored).toBeDefined()
    // gpui-markdown-editor is `publish = false`, so no cargo tool will ever
    // emit it — this section is the only route by which its upstream authors
    // are attributed at all.
    expect(vendored).toContain(`gpui-markdown-editor`)
    expect(vendored).toContain(`Velotype`)
    expect(vendored).toContain(read(`apps/desktop/crates/ui/NOTICE`).trim())
    // EXP-1273: the cua driver is gone with its curated reproduction.
    expect(vendored).not.toContain(`cua-driver-sdk`)
    expect(vendored).not.toContain(`Cua AI, Inc.`)
  })

  it(`the desktop records what was patched out`, () => {
    const excluded = section(
      notices.desktop,
      `Components deliberately not included`
    )
    expect(excluded).toBeDefined()
    expect(excluded).toContain(`ztracing`)
    expect(excluded).toContain(`zlog`)
    // The claim has to stay true: zlog must not reappear in the lockfile.
    expect(read(`apps/desktop/Cargo.lock`)).not.toContain(`name = "zlog"`)
  })
})

// ---------------------------------------------------------------------------
// Dedup
// ---------------------------------------------------------------------------

describe(`aggregation`, () => {
  it(`licence bodies are shared, copyright lines are not`, () => {
    // Crates that ship a byte-identical body share one reproduction. Crates
    // that ship their own MIT file with their own copyright line inside it
    // cannot be merged — that line is exactly what MIT requires us to carry,
    // and editing it out of the body would be altering reproduced terms.
    const pairs = rust.components.flatMap((c) => c.texts)
    const distinct = new Set(pairs.map((t) => `${t.spdx}\u0000${t.body}`))
    expect(distinct.size, `bodies are not being shared at all`).toBeLessThan(
      pairs.length / 2
    )

    const oss = section(notices.desktop, `Open-source components`)!
    // The single largest group must actually cover many crates.
    expect(oss).toMatch(/Applies to the following \d\d+ components:/)
    // …and distinct copyright holders are still named individually.
    expect(oss).toContain(`Zed Industries, Inc.`)
    expect(oss).toContain(`Longbridge`)
  })

  it(`the desktop notice stays a sane size`, () => {
    // Without body deduplication this file is several megabytes of
    // near-identical MIT and Apache text.
    expect(notices.desktop.length).toBeLessThan(2_500_000)
    expect(notices.desktop.length).toBeGreaterThan(200_000)
  })
})
