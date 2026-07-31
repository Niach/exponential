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
}

const inventory = (file: string): Inventory =>
  JSON.parse(readPkg(`inventory/${file}`)) as Inventory

const rust = inventory(`rust.json`)
const npmWeb = inventory(`npm-web.json`)
const npmMarketing = inventory(`npm-marketing.json`)
const ios = inventory(`ios.json`)
const android = inventory(`android.json`)

/** Where each client's notice is committed. Mirrors OUTPUTS in generate.ts. */
const OUTPUTS = {
  desktop: `apps/desktop/assets/licenses/NOTICES.txt`,
  web: `apps/web/public/NOTICES.txt`,
  marketing: `apps/marketing/public/NOTICES.txt`,
  ios: `apps/ios/Exponential/Resources/NOTICES.txt`,
  android: `apps/android/app/src/main/assets/NOTICES.txt`,
} as const

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
  // bun.lock is JSON with trailing commas. Strip them rather than pull in a
  // parser: the file is machine-written, so the shape is predictable.
  const bunLock = JSON.parse(
    read(`bun.lock`).replace(/,(\s*[}\]])/g, `$1`)
  ) as {
    workspaces: Record<string, { dependencies?: Record<string, string> }>
    packages: Record<string, [string, string, Record<string, unknown>, string]>
  }

  /** name -> every version bun.lock resolves for it. */
  const resolved = new Map<string, Set<string>>()
  for (const entry of Object.values(bunLock.packages)) {
    const spec = entry[0]
    // Names may be scoped (`@scope/name@1.2.3`), so split at the LAST `@`
    // that is not the leading one.
    const at = spec.lastIndexOf(`@`)
    if (at <= 0) continue
    const name = spec.slice(0, at)
    const version = spec.slice(at + 1)
    const versions = resolved.get(name) ?? new Set<string>()
    versions.add(version)
    resolved.set(name, versions)
  }

  const cases = [
    { scope: `apps/web`, inv: npmWeb },
    { scope: `apps/marketing`, inv: npmMarketing },
  ]

  it.each(cases)(`$scope: nothing attributed has left bun.lock`, ({ inv }) => {
    const stale = inv.components
      .filter((c) => {
        const known = resolved.get(c.name)
        return known !== undefined && !known.has(c.version)
      })
      .map((c) => `${c.name}@${c.version}`)
    expect(stale, `versions drifted from bun.lock`).toEqual([])
  })

  it.each(cases)(`$scope: every production dependency is covered`, ({ scope, inv }) => {
    const direct = Object.keys(bunLock.workspaces[scope]?.dependencies ?? {})
    expect(direct.length, `${scope} declares no dependencies?`).toBeGreaterThan(0)
    const attributed = new Set(inv.components.map((c) => c.name))
    const missing = direct.filter(
      (name) => !name.startsWith(`@exp/`) && !attributed.has(name)
    )
    expect(
      missing,
      `${scope} production deps missing from the inventory — run ` +
        `\`bun run --filter @exp/licenses collect:npm\``
    ).toEqual([])
  })

  it.each(cases)(`$scope: no workspace package is attributed`, ({ inv }) => {
    // Our own code is covered by the repository LICENSE, not by a notice entry.
    expect(inv.components.filter((c) => c.name.startsWith(`@exp/`))).toEqual([])
  })

  it(`platform-gated packages are attributed on every host`, () => {
    // ~55 packages in the closure are os/cpu-gated (@esbuild/*, @rollup/*,
    // lightningcss-*, @remotion/compositor-*). Only the current host's copy is
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
    const elected = new Set(rust.components.flatMap((c) => c.licenses))
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
    const elected = new Set(rust.components.flatMap((c) => c.licenses))
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
    for (const client of [`desktop`, `web`, `marketing`, `android`, `ios`] as const) {
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
    for (const mark of [`Apple logo`, `Google "G" logo`, `Claude logo`, `Codex logo`, `Pi logo`]) {
      expect(trademarks).toContain(mark)
    }
  })

  it(`non-OSS components are separated from the open-source aggregate`, () => {
    // docs/third-party-licences.md: marketing carries Remotion, iOS and Android
    // carry Google's closed binaries, and web + desktop carry nothing — the
    // section must be ABSENT rather than empty.
    for (const client of [`marketing`, `ios`, `android`] as const) {
      const commercial = section(notices[client], `Commercially licensed components`)
      expect(commercial, `${client} should have a commercial section`).toBeDefined()
      expect(flat(commercial!)).toContain(`NOT open source`)
    }
    for (const client of [`desktop`, `web`] as const) {
      // docs/third-party-licences.md: "the section should be absent rather
      // than empty". The two web packages that declare no licence at all
      // (@better-fetch/fetch, creem) are both MIT — see curated/overrides.ts.
      expect(
        section(notices[client], `Commercially licensed components`),
        `${client} should have no commercial section`
      ).toBeUndefined()
    }
  })

  it(`marketing reproduces the Remotion licence verbatim`, () => {
    const commercial = section(notices.marketing, `Commercially licensed components`)!
    const licence = read(`docs/licences/remotion-LICENSE.txt`)
    const distinctive = `a for-profit organization with up to 3 employees`
    expect(licence).toContain(distinctive)
    expect(commercial).toContain(distinctive)
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
    for (const client of [`desktop`, `marketing`] as const) {
      const fonts = section(notices[client], `Bundled fonts`)
      expect(fonts, `${client} ships font binaries`).toBeDefined()
      expect(fonts).toContain(`SIL OPEN FONT LICENSE Version 1.1`)
    }
    // The web app links Google Fonts at runtime and distributes no font binary,
    // so it has no OFL obligation and must not claim one.
    expect(section(notices.web, `Bundled fonts`)).toBeUndefined()
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
