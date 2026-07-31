#!/usr/bin/env bun
// EXP-375 — the Android collector.
//
// WHY A GRADLE REPORT AND NOT `libs.versions.toml`
// ------------------------------------------------
// `apps/android/gradle/libs.versions.toml` names ~49 libraries, but `compose-bom`
// and `firebase-bom` expand to 200+ artifacts the TOML never spells out. A notice
// must cover what the APK/AAB actually SHIPS, so this collector reads the
// transitively resolved `productionRelease` runtime graph via the
// `com.jaredsburrows.license` gradle plugin (wired in `apps/android/app/build.gradle.kts`).
//
//   cd apps/android && ./gradlew licenseProductionReleaseReport
//   bun run --filter @exp/licenses collect:android
//
// The plugin is configured JSON-only and copies NOTHING into `assets/` — the
// merge step (`scripts/generate.ts`) owns `apps/android/app/src/main/assets/NOTICES.txt`
// and a plugin writing there would fight the drift gate.
//
// LICENCE BODIES
// --------------
// A Maven POM declares a licence NAME + URL, never a body. Bodies therefore come
// from the canonical templates in `packages/licenses/texts/spdx/` (`source:
// "spdx-template"`). This script NEVER authors licence text: an SPDX id with no
// template is reported on stderr and the component ships `texts: []`.
//
// Each artifact's OWN copyright lines are preserved separately, lifted verbatim
// from `META-INF/{LICENSE,NOTICE,COPYRIGHT}*` inside the resolved jar/aar (found
// through the local Gradle module cache, which a successful report run has by
// definition just populated). Nothing is synthesised from POM `<inceptionYear>`
// / `<developers>` — a constructed copyright line is not a copyright notice.
//
// NON-OSS COMPONENTS
// ------------------
// Some Google artifacts declare proprietary terms rather than an OSS licence
// (see `docs/third-party-licences.md`, "Closed-source Google binaries"). They are
// emitted with `licenses: []` / `texts: []` and a `homepage` pointing at the
// terms, which routes them into the merge step's separate "Commercially licensed
// components" section — folding them into the Apache aggregate would assert terms
// that were never granted.
//
// DETERMINISM
// -----------
// Output is sorted by (name, version) with codepoint comparison and contains no
// timestamps, absolute paths, hostnames or build directories. Any surprise (a
// missing report, an unmapped licence name, a malformed coordinate) is FATAL —
// this file is a legal artifact and a partial one is worse than none.

import {
  existsSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { execFileSync } from "node:child_process"
import { dirname, join, resolve } from "node:path"
import { homedir } from "node:os"

import {
  INVENTORY_COMMENT,
  compareComponents,
  type Component,
  type Inventory,
  type LicenceText,
} from "../src/schema"

const HERE = dirname(new URL(import.meta.url).pathname)
const PKG = resolve(HERE, `..`)
const REPO = resolve(PKG, `..`, `..`)

const REPORT = join(
  REPO,
  `apps/android/app/build/reports/licenses/licenseProductionReleaseReport.json`
)
const TEXTS = join(PKG, `texts/spdx`)
const OUT = join(PKG, `inventory/android.json`)
const GRADLE_MODULES = join(homedir(), `.gradle/caches/modules-2/files-2.1`)

const COLLECTOR = `packages/licenses/scripts/collect-android.ts`

/** One entry of the gradle-license-plugin JSON report. */
interface ReportEntry {
  project?: string | null
  version?: string | null
  url?: string | null
  dependency?: string | null
  licenses?: { license?: string | null; license_url?: string | null }[] | null
}

const fail = (message: string): never => {
  console.error(`collect-android: ${message}`)
  process.exit(1)
}

// ---------------------------------------------------------------------------
// Licence name -> SPDX id.
//
// Maven POMs spell the same licence a dozen ways, so the mapping is an explicit
// table keyed on the whitespace-collapsed, lowercased declared name. It is
// deliberately NOT a fuzzy matcher: an unrecognised name is fatal, because a
// licence we silently failed to classify is a licence we silently failed to
// reproduce.
// ---------------------------------------------------------------------------
const SPDX_BY_NAME: Record<string, string> = {
  "apache 2.0": `Apache-2.0`,
  "apache license 2.0": `Apache-2.0`,
  "apache license, version 2.0": `Apache-2.0`,
  "apache software license, version 2.0": `Apache-2.0`,
  "apache-2.0": `Apache-2.0`,
  "the apache license, version 2.0": `Apache-2.0`,
  "the apache software license, version 2.0": `Apache-2.0`,
  "bsd-2-clause": `BSD-2-Clause`,
  "bsd 2-clause license": `BSD-2-Clause`,
  "the bsd 2-clause license": `BSD-2-Clause`,
  "bsd-3-clause": `BSD-3-Clause`,
  "bsd 3-clause license": `BSD-3-Clause`,
  "the bsd 3-clause license": `BSD-3-Clause`,
  isc: `ISC`,
  "isc license": `ISC`,
  mit: `MIT`,
  "mit license": `MIT`,
  "the mit license": `MIT`,
  "eclipse public license 1.0": `EPL-1.0`,
  "eclipse public license - v 1.0": `EPL-1.0`,
  "eclipse public license 2.0": `EPL-2.0`,
  "eclipse public license - v 2.0": `EPL-2.0`,
  "mozilla public license 2.0": `MPL-2.0`,
  "mozilla public license, version 2.0": `MPL-2.0`,
  "common development and distribution license 1.0": `CDDL-1.0`,
  "cddl 1.0": `CDDL-1.0`,
}

// ---------------------------------------------------------------------------
// Proprietary terms that are NOT open source. Keyed the same way; the value is
// the canonical URL of the terms, used as the component's `homepage` since these
// artifacts declare no project URL. `licenses: []` is the signal the merge step
// keys on.
// ---------------------------------------------------------------------------
const PROPRIETARY_BY_NAME: Record<string, string> = {
  "play core software development kit terms of service": `https://developer.android.com/guide/playcore/license`,
  "android software development kit license": `https://developer.android.com/studio/terms.html`,
  "android software development kit license agreement": `https://developer.android.com/studio/terms.html`,
}

const normaliseName = (name: string): string =>
  name.replace(/\s+/g, ` `).trim().toLowerCase()

/** Normalise a reproduced text: BOM out, `\n` newlines, no trailing blanks. */
const normaliseText = (raw: string): string =>
  raw
    .replace(/^﻿/, ``)
    .replace(/\r\n?/g, `\n`)
    .split(`\n`)
    .map((line) => line.replace(/[ \t]+$/, ``))
    .join(`\n`)
    .replace(/\n+$/, ``)

// ---------------------------------------------------------------------------
// SPDX templates.
// ---------------------------------------------------------------------------
const templateCache = new Map<string, string | null>()
const missingTemplates = new Map<string, string[]>()

const templateFor = (spdx: string, artifact: string): string | null => {
  if (!templateCache.has(spdx)) {
    const path = join(TEXTS, `${spdx}.txt`)
    templateCache.set(
      spdx,
      existsSync(path) ? normaliseText(readFileSync(path, `utf8`)) : null
    )
  }
  const body = templateCache.get(spdx) ?? null
  if (body === null) {
    const seen = missingTemplates.get(spdx) ?? []
    seen.push(artifact)
    missingTemplates.set(spdx, seen)
  }
  return body
}

// ---------------------------------------------------------------------------
// Copyright extraction.
//
// The Gradle module cache lays artifacts out as
// `<group>/<module>/<version>/<sha1>/<file>`. Indexing it once is far cheaper
// than 189 directory probes, and it lets Kotlin-Multiplatform artifacts resolve:
// `io.ktor:ktor-client-core` publishes its android bytes as `ktor-client-core-jvm`.
//
// Modules with no jar/aar at all (BOM POMs, KMP `-internal` metadata redirects)
// are expected and simply contribute no copyright line.
// ---------------------------------------------------------------------------
const KMP_SUFFIXES = [``, `-android`, `-jvm`, `-jvm18`, `-desktop`]

const buildArtifactIndex = (): Map<string, string> => {
  const index = new Map<string, string>()
  if (!existsSync(GRADLE_MODULES)) return index
  const dirs = (path: string): string[] => {
    try {
      return readdirSync(path).filter((entry) => {
        try {
          return statSync(join(path, entry)).isDirectory()
        } catch {
          return false
        }
      })
    } catch {
      return []
    }
  }
  for (const group of dirs(GRADLE_MODULES)) {
    const groupDir = join(GRADLE_MODULES, group)
    for (const module of dirs(groupDir)) {
      const moduleDir = join(groupDir, module)
      for (const version of dirs(moduleDir)) {
        const versionDir = join(moduleDir, version)
        for (const sha of dirs(versionDir)) {
          let files: string[] = []
          try {
            files = readdirSync(join(versionDir, sha))
          } catch {
            continue
          }
          for (const file of files) {
            if (!file.endsWith(`.jar`) && !file.endsWith(`.aar`)) continue
            if (file.includes(`-sources`) || file.includes(`-javadoc`)) continue
            index.set(
              `${group}:${module}:${version}`,
              join(versionDir, sha, file)
            )
          }
        }
      }
    }
  }
  return index
}

const META_INF = /^META-INF\/(LICENSE|NOTICE|COPYRIGHT)[^/]*$/i
/**
 * A real copyright notice starts the line and carries a year or a ©. This keeps
 * out markdown headings like `## Copyright` and prose that merely mentions the
 * word.
 */
const COPYRIGHT_LINE = /^(?:copyright|\(c\)|©)\b/i
const HAS_HOLDER = /\d{4}|©/

const zipEntries = (archive: string): string[] => {
  try {
    return execFileSync(`unzip`, [`-Z1`, archive], {
      encoding: `utf8`,
      maxBuffer: 64 * 1024 * 1024,
    })
      .split(`\n`)
      .map((line) => line.trim())
      .filter(Boolean)
  } catch {
    return []
  }
}

const zipRead = (archive: string, entry: string): string | null => {
  try {
    return execFileSync(`unzip`, [`-p`, archive, entry], {
      encoding: `utf8`,
      maxBuffer: 32 * 1024 * 1024,
    })
  } catch {
    return null
  }
}

const copyrightsFor = (archive: string): string[] => {
  const out: string[] = []
  const seen = new Set<string>()
  // Sorted so the order does not depend on the archive's central directory.
  for (const entry of zipEntries(archive)
    .filter((e) => META_INF.test(e))
    .sort()) {
    const body = zipRead(archive, entry)
    if (body === null) continue
    for (const raw of normaliseText(body).split(`\n`)) {
      const line = raw
        .trim()
        .replace(/^[*#/\s]+/, ``)
        .trim()
      if (line.length === 0 || line.length > 300) continue
      if (!COPYRIGHT_LINE.test(line) || !HAS_HOLDER.test(line)) continue
      if (seen.has(line)) continue
      seen.add(line)
      out.push(line)
    }
  }
  return out
}

// ---------------------------------------------------------------------------
// Main.
// ---------------------------------------------------------------------------
const main = (): void => {
  if (!existsSync(REPORT)) {
    fail(
      `no licence report at apps/android/app/build/reports/licenses/licenseProductionReleaseReport.json.\n` +
        `  Generate it first:  cd apps/android && ./gradlew licenseProductionReleaseReport`
    )
  }

  let report: ReportEntry[]
  try {
    report = JSON.parse(readFileSync(REPORT, `utf8`)) as ReportEntry[]
  } catch (error) {
    return void fail(
      `licence report is not valid JSON: ${(error as Error).message}`
    )
  }
  if (!Array.isArray(report) || report.length === 0) {
    fail(`licence report is empty — did the gradle resolve actually run?`)
  }

  const index = buildArtifactIndex()
  if (index.size === 0) {
    fail(
      `the Gradle module cache at ~/.gradle/caches/modules-2/files-2.1 is empty or absent.\n` +
        `  Copyright notices are lifted from the resolved artifacts, so a run without it\n` +
        `  would silently drop them. Run the gradle report task first.`
    )
  }

  const unmapped: string[] = []
  const noArtifact: string[] = []
  const proprietary: string[] = []
  const components: Component[] = []

  for (const entry of report) {
    const coordinate = (entry.dependency ?? ``).trim()
    const parts = coordinate.split(`:`)
    if (parts.length !== 3 || parts.some((part) => part.length === 0)) {
      fail(
        `malformed dependency coordinate in the report: ${JSON.stringify(entry.dependency)}`
      )
    }
    const [group, artifact, version] = parts
    const name = `${group}:${artifact}`

    const declaredNames = (entry.licenses ?? [])
      .map((licence) => (licence.license ?? ``).replace(/\s+/g, ` `).trim())
      .filter(Boolean)
    if (declaredNames.length === 0) {
      fail(
        `${coordinate} declares no licence at all — resolve it by hand before regenerating`
      )
    }

    const spdx: string[] = []
    let termsUrl: string | null = null
    for (const declared of declaredNames) {
      const key = normaliseName(declared)
      if (key in PROPRIETARY_BY_NAME) {
        termsUrl = PROPRIETARY_BY_NAME[key]
        continue
      }
      const id = SPDX_BY_NAME[key]
      if (!id) {
        unmapped.push(`${coordinate} — ${declared}`)
        continue
      }
      if (!spdx.includes(id)) spdx.push(id)
    }

    // A component may not be half proprietary and half OSS; the notice would
    // have to pick a section and either choice would misstate the terms.
    if (termsUrl !== null && spdx.length > 0) {
      fail(
        `${coordinate} mixes proprietary terms with an OSS licence: ${declaredNames.join(`, `)}`
      )
    }

    const isProprietary = termsUrl !== null
    if (isProprietary) proprietary.push(coordinate)

    spdx.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))

    const texts: LicenceText[] = []
    for (const id of spdx) {
      const body = templateFor(id, coordinate)
      if (body === null) continue
      texts.push({ spdx: id, source: `spdx-template`, body })
    }

    let copyright: string[] = []
    let archive: string | null = null
    for (const suffix of KMP_SUFFIXES) {
      const hit = index.get(`${group}:${artifact}${suffix}:${version}`)
      if (hit) {
        archive = hit
        break
      }
    }
    if (archive) copyright = copyrightsFor(archive)
    else noArtifact.push(coordinate)

    const homepage = isProprietary
      ? termsUrl!
      : (entry.url ?? ``).trim() || undefined

    const component: Component = {
      name,
      version,
      // `declared` is an SPDX expression, matching the npm/rust/swift collectors
      // — a POM declares a prose licence NAME, so it is mapped through
      // SPDX_BY_NAME first. Proprietary terms have no SPDX id, so they keep the
      // declared name verbatim; that string is what the notice will print.
      declared: isProprietary
        ? declaredNames.join(` AND `)
        : spdx.join(` AND `),
      licenses: spdx,
      ...(homepage ? { homepage } : {}),
      ...(copyright.length > 0 ? { copyright } : {}),
      texts: isProprietary ? [] : texts,
    }
    components.push(component)
  }

  if (unmapped.length > 0) {
    fail(
      `unmapped licence names — add them to SPDX_BY_NAME or PROPRIETARY_BY_NAME,\n` +
        `never guess a body:\n  ${unmapped.join(`\n  `)}`
    )
  }

  components.sort(compareComponents)

  const duplicates = components.filter(
    (component, i) =>
      i > 0 &&
      components[i - 1].name === component.name &&
      components[i - 1].version === component.version
  )
  if (duplicates.length > 0) {
    fail(
      `duplicate components in the report: ${duplicates.map((d) => `${d.name}:${d.version}`).join(`, `)}`
    )
  }

  const inventory: Inventory = {
    $comment: INVENTORY_COMMENT,
    ecosystem: `android`,
    scope: `android`,
    collector: COLLECTOR,
    components,
  }

  writeFileSync(OUT, `${JSON.stringify(inventory, null, 2)}\n`)

  for (const [spdx, artifacts] of [...missingTemplates.entries()].sort()) {
    console.error(
      `MISSING TEMPLATE: ${spdx} (needed by ${artifacts.sort().join(`, `)})`
    )
  }
  if (noArtifact.length > 0) {
    console.error(
      `note: no jar/aar in the Gradle cache for ${noArtifact.length} module(s) — expected for BOM ` +
        `POMs and KMP metadata modules, which carry no META-INF:\n  ${noArtifact.sort().join(`\n  `)}`
    )
  }
  console.error(
    `collect-android: ${components.length} components, ` +
      `${proprietary.length} under proprietary terms (${proprietary.sort().join(`, `)})`
  )
}

main()
