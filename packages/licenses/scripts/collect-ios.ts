#!/usr/bin/env bun
// EXP-375 — the iOS (SwiftPM) licence collector.
//
// Input:  apps/ios/Tuist/Package.resolved
// Output: packages/licenses/inventory/ios.json
//
// SwiftPM has no metadata field for a licence — a `Package.swift` declares
// nothing of the sort — so unlike cargo/npm there is no declaration to read.
// The only ground truth is the licence file in the dependency's own repository,
// and the only way to be sure we read the one that belongs to the code we ship
// is to fetch it **at the pinned revision**:
//
//   https://raw.githubusercontent.com/<owner>/<repo>/<revision>/<candidate>
//
// That is what makes this collector deterministic and re-runnable: the same
// `Package.resolved` yields the same bytes forever, because a git revision is
// immutable. Nothing here reads the clock, the filesystem outside the two paths
// above, or the machine's identity — run it on macOS or on an ubuntu runner and
// the output file is byte-identical.
//
// The SPDX id is *sniffed* from the fetched body via distinctive phrases, never
// guessed from the package name. A body that matches nothing is a hard failure,
// not an "UNKNOWN" row: a notices file that quietly under-reports terms is worse
// than one that fails to build.

import {
  INVENTORY_COMMENT,
  compareComponents,
  type Component,
  type Inventory,
  type LicenceText,
} from "../src/schema.ts"

const PACKAGE_RESOLVED = new URL(
  `../../../apps/ios/Tuist/Package.resolved`,
  import.meta.url
)
const OUTPUT = new URL(`../inventory/ios.json`, import.meta.url)
const COLLECTOR = `packages/licenses/scripts/collect-ios.ts`

/** Tried in this order; the first 200 wins. */
const CANDIDATES = [
  `LICENSE`,
  `LICENSE.md`,
  `LICENSE.txt`,
  `LICENCE`,
  `COPYING`,
  `NOTICE`,
] as const

// ---------------------------------------------------------------------------
// Non-OSS components — docs/third-party-licences.md, "Closed-source Google
// binaries — mobile only" (EXP-262).
//
// Both repos below *do* carry an Apache-2.0 `LICENSE` file, and sniffing it is
// exactly the trap this table exists to avoid: that file covers the SwiftPM
// wrapper, not the closed-source `.xcframework` binaries the store build
// actually links. Emitting them as Apache-2.0 would assert a grant Google never
// made. So they are emitted with `licenses: []` and `texts: []` — there is no
// licence body to reproduce — which is the signal `scripts/generate.ts` uses to
// route them into the dedicated "Commercially licensed components" section
// instead of the MIT/Apache aggregate.
//
// `declared` carries the terms name from that doc's table verbatim.
// ---------------------------------------------------------------------------
const CLOSED_SOURCE: Record<string, { declared: string; homepage: string }> = {
  googleappmeasurement: {
    declared: `Closed-source binary framework under Google's own terms`,
    homepage: `https://firebase.google.com/terms`,
  },
  [`google-ads-on-device-conversion-ios-sdk`]: {
    declared: `Closed-source binary, Google Ads terms`,
    homepage: `https://developers.google.com/terms`,
  },
}

interface Pin {
  identity: string
  kind: string
  location: string
  state: { revision: string; version: string }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/**
 * `Package.resolved` is Swift-flavoured JSON — spaces before every colon — but
 * it is still valid JSON, so a BOM strip plus `JSON.parse` is the whole parser.
 * Everything past that point is validation: anything unexpected throws rather
 * than being skipped, because a silently dropped pin is a missing notice.
 */
const parsePackageResolved = (raw: string): Pin[] => {
  const doc = JSON.parse(raw.replace(/^﻿/, ``)) as { pins?: unknown }
  if (!Array.isArray(doc.pins)) {
    throw new Error(`Package.resolved has no \`pins\` array`)
  }
  return doc.pins.map((entry, index) => {
    const pin = entry as Partial<Pin>
    if (typeof pin.identity !== `string` || pin.identity === ``) {
      throw new Error(`pins[${index}] has no \`identity\``)
    }
    if (pin.kind !== `remoteSourceControl`) {
      throw new Error(
        `pins[${index}] (${pin.identity}) has unsupported kind \`${String(pin.kind)}\` — ` +
          `this collector only knows how to fetch remote source-control pins`
      )
    }
    if (typeof pin.location !== `string`) {
      throw new Error(`pins[${index}] (${pin.identity}) has no \`location\``)
    }
    const revision = pin.state?.revision
    const version = pin.state?.version
    if (typeof revision !== `string` || revision === ``) {
      throw new Error(`${pin.identity} has no \`state.revision\` to pin to`)
    }
    if (typeof version !== `string` || version === ``) {
      throw new Error(
        `${pin.identity} is pinned to a branch/revision without a \`state.version\` — ` +
          `a notices entry needs a resolved version`
      )
    }
    return {
      identity: pin.identity,
      kind: pin.kind,
      location: pin.location,
      state: { revision, version },
    }
  })
}

/** `https://github.com/google/promises.git` -> `google/promises`. */
const slugFromLocation = (identity: string, location: string): string => {
  const match = /^https:\/\/github\.com\/([^/]+)\/(.+?)(?:\.git)?\/?$/.exec(
    location
  )
  if (!match) {
    throw new Error(
      `${identity}: cannot derive owner/repo from \`${location}\` — ` +
        `this collector only fetches from github.com`
    )
  }
  return `${match[1]}/${match[2]}`
}

// ---------------------------------------------------------------------------
// Normalisation — the same text must produce the same bytes on every host.
// ---------------------------------------------------------------------------

const normaliseBody = (raw: string): string =>
  raw
    .replace(/^﻿/, ``)
    .replace(/\r\n?/g, `\n`)
    .split(`\n`)
    .map((line) => line.replace(/[ \t]+$/, ``))
    .join(`\n`)
    .replace(/\n+$/, ``)

// ---------------------------------------------------------------------------
// Sniffing
// ---------------------------------------------------------------------------

/** Whitespace-insensitive, case-insensitive form, so line wrapping cannot hide a phrase. */
const flatten = (text: string): string =>
  text.replace(/\s+/g, ` `).trim().toLowerCase()

const has = (flat: string, ...phrases: string[]): boolean =>
  phrases.every((phrase) => flat.includes(flatten(phrase)))

/**
 * Identify every licence whose *body* is present in the fetched file. Several of
 * these files are composites (a project that vendored code under a second
 * licence appends it below a separator), which is why this returns a set rather
 * than one id.
 *
 * Each rule matches on phrases that appear in that licence and no other — never
 * on a title line alone, which is what a package's own README would trip.
 */
const sniff = (body: string): string[] => {
  const flat = flatten(body)
  const found = new Set<string>()

  const apache = has(
    flat,
    `Apache License`,
    `Version 2.0, January 2004`,
    `TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION`
  )
  if (apache) {
    // Apple's Swift projects append the SPDX `Swift-exception` verbatim. It only
    // ever *adds* rights (it waives the Apache attribution clauses), but the
    // notice has to say which terms we actually hold.
    const swiftException = has(
      flat,
      `you may redistribute such product without providing attribution as would otherwise be required by Sections 4(a), 4(b) and 4(d) of the License`
    )
    found.add(swiftException ? `Apache-2.0 WITH Swift-exception` : `Apache-2.0`)
  }

  if (
    has(
      flat,
      `Permission is hereby granted, free of charge, to any person obtaining a copy`,
      `without restriction, including without limitation the rights`,
      `The above copyright notice and this permission notice shall be included in`
    )
  ) {
    found.add(`MIT`)
  }

  if (
    has(
      flat,
      `Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted`
    )
  ) {
    found.add(`ISC`)
  }

  const redistribution = has(
    flat,
    `Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met`
  )
  if (redistribution) {
    // The third clause is the only thing separating BSD-3 from BSD-2, so test it
    // first and never emit both.
    if (
      has(
        flat,
        `may be used to endorse or promote products derived from this software`
      )
    ) {
      found.add(`BSD-3-Clause`)
    } else if (
      has(flat, `Redistributions in binary form must reproduce the above`)
    ) {
      found.add(`BSD-2-Clause`)
    }
  }

  if (
    has(
      flat,
      `The origin of this software must not be misrepresented`,
      `Altered source versions must be plainly marked as such`
    )
  ) {
    found.add(`zlib`)
  }

  return [...found].sort()
}

/**
 * Copyright holders, which MIT/BSD/ISC require us to reproduce per component.
 * A year is mandatory, which drops both the Apache template's unfilled
 * `Copyright [yyyy] [name of copyright owner]` placeholder and the prose
 * "copyright notice ..." lines inside the grant clauses.
 */
const copyrightLines = (body: string): string[] => {
  const seen = new Set<string>()
  for (const raw of body.split(`\n`)) {
    const line = raw.trim()
    if (!/^copyright\b/i.test(line)) continue
    if (!/\b(19|20)\d{2}\b/.test(line)) continue
    if (line.includes(`[yyyy]`)) continue
    seen.add(line)
  }
  return [...seen]
}

// ---------------------------------------------------------------------------
// Fetching
// ---------------------------------------------------------------------------

const fetchWithRetry = async (url: string): Promise<Response> => {
  let lastError: unknown
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      const res = await fetch(url)
      // 404 is a real answer ("this candidate does not exist"); 5xx and 429 are not.
      if (res.status < 500 && res.status !== 429) return res
      lastError = new Error(`HTTP ${res.status}`)
    } catch (error) {
      lastError = error
    }
    await Bun.sleep(500 * (attempt + 1))
  }
  throw new Error(
    `${url}: ${lastError instanceof Error ? lastError.message : String(lastError)}`
  )
}

interface FetchedLicence {
  candidate: string
  body: string
}

const fetchLicence = async (
  identity: string,
  slug: string,
  revision: string
): Promise<FetchedLicence> => {
  for (const candidate of CANDIDATES) {
    const url = `https://raw.githubusercontent.com/${slug}/${revision}/${candidate}`
    const res = await fetchWithRetry(url)
    if (res.status === 404) continue
    if (!res.ok)
      throw new Error(`${identity}: ${url} returned HTTP ${res.status}`)
    const body = normaliseBody(await res.text())
    if (body === ``) continue
    return { candidate, body }
  }
  throw new Error(
    `${identity}: none of ${CANDIDATES.join(`, `)} exists at ${slug}@${revision} — ` +
      `find the terms by hand and add the component to docs/third-party-licences.md`
  )
}

// ---------------------------------------------------------------------------
// Collection
// ---------------------------------------------------------------------------

const collect = async (pin: Pin): Promise<Component> => {
  const slug = slugFromLocation(pin.identity, pin.location)
  if (Object.hasOwn(CLOSED_SOURCE, pin.identity)) {
    const closed = CLOSED_SOURCE[pin.identity]!
    return {
      name: pin.identity,
      version: pin.state.version,
      declared: closed.declared,
      licenses: [],
      homepage: closed.homepage,
      texts: [],
    }
  }

  const { candidate, body } = await fetchLicence(
    pin.identity,
    slug,
    pin.state.revision
  )
  const licenses = sniff(body)
  if (licenses.length === 0) {
    throw new Error(
      `${pin.identity}: ${candidate} at ${pin.state.revision} matches no known licence — ` +
        `read it and either teach \`sniff()\` the phrase or record it in docs/third-party-licences.md`
    )
  }

  // No pin in this graph offers a choice of licences, so nothing elects a branch
  // and `election` stays unset. A composite file (Apache-2.0 + a vendored MIT
  // block, say) is a conjunction: every licence in it binds us.
  const declared = licenses.join(` AND `)
  // One body per file, not per id: where a single file carries two licences the
  // reproduced text *is* the text of the conjunction, and slicing it apart would
  // mean editing licence text.
  const texts: LicenceText[] = [{ spdx: declared, source: candidate, body }]

  const copyright = copyrightLines(body)
  return {
    name: pin.identity,
    version: pin.state.version,
    declared,
    licenses,
    homepage: `https://github.com/${slug}`,
    ...(copyright.length > 0 ? { copyright } : {}),
    texts,
  }
}

const main = async (): Promise<void> => {
  const pins = parsePackageResolved(await Bun.file(PACKAGE_RESOLVED).text())

  const components: Component[] = []
  const failures: string[] = []
  for (const pin of pins) {
    try {
      components.push(await collect(pin))
    } catch (error) {
      failures.push(error instanceof Error ? error.message : String(error))
    }
  }

  // Never write a partial inventory: a notices file missing a component is a
  // licence breach that looks like a successful build.
  if (failures.length > 0) {
    for (const failure of failures) console.error(`collect-ios: ${failure}`)
    console.error(
      `collect-ios: ${failures.length}/${pins.length} pins failed; inventory/ios.json not written`
    )
    process.exit(1)
  }

  components.sort(compareComponents)

  const inventory: Inventory = {
    $comment: INVENTORY_COMMENT,
    ecosystem: `swift`,
    scope: `ios`,
    collector: COLLECTOR,
    components,
  }

  await Bun.write(OUTPUT, `${JSON.stringify(inventory, null, 2)}\n`)
  console.error(
    `collect-ios: ${components.length} components from ${pins.length} pins`
  )
}

await main()
