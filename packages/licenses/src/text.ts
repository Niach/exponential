// EXP-375 — reading, normalising and identifying licence bodies on disk.
//
// Shared by every collector so that a body found in a crate, an npm package or
// an SPDX template normalises identically and therefore DEDUPES against the
// others. Without that, the desktop notice is ~6.4 MB of near-identical MIT
// texts instead of ~1.4 MB.

import { existsSync, readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"

import { byCodepoint } from "./schema"

/**
 * Canonical form of a reproduced licence body: no BOM, LF endings, no trailing
 * whitespace on any line, exactly one trailing newline. Nothing inside the text
 * is rewritten — altering reproduced terms is what the licences forbid.
 */
export const normaliseBody = (text: string): string =>
  text
    .replace(/^﻿/, ``)
    .replace(/\r\n?/g, `\n`)
    .split(`\n`)
    .map((line) => line.replace(/[ \t]+$/, ``))
    .join(`\n`)
    .replace(/\n+$/, ``) + `\n`

/** Filenames that plausibly hold a licence, in no particular order. */
const LICENCE_FILE = /^(licen[cs]e|copying|notice|unlicen[cs]e|ofl)([-._].*)?$/i

/**
 * Map a licence filename to the SPDX id it is the text of, when the name says
 * so. `LICENSE-MIT` -> MIT, `LICENSE.APACHE2` -> Apache-2.0. Returns undefined
 * for a bare `LICENSE`, which has to be sniffed from its contents instead.
 */
export function spdxFromFilename(file: string): string | undefined {
  const base = file.toLowerCase()
  if (/(^|[-._])(mit)([-._]|$)/.test(base)) return `MIT`
  if (/(^|[-._])(apache|asl)/.test(base)) return `Apache-2.0`
  if (/(^|[-._])(isc)([-._]|$)/.test(base)) return `ISC`
  if (/(^|[-._])(mpl|mozilla)/.test(base)) return `MPL-2.0`
  if (/(^|[-._])(bsd-?3|bsd3)/.test(base)) return `BSD-3-Clause`
  if (/(^|[-._])(bsd-?2|bsd2)/.test(base)) return `BSD-2-Clause`
  if (/(^|[-._])(zlib)([-._]|$)/.test(base)) return `Zlib`
  if (/(^|[-._])(unlicense)([-._]|$)/.test(base)) return `Unlicense`
  if (/(^|[-._])(cc0)/.test(base)) return `CC0-1.0`
  if (/(^|[-._])(boost|bsl)/.test(base)) return `BSL-1.0`
  if (/(^|[-._])(ofl|sil)/.test(base)) return `OFL-1.1`
  return undefined
}

/**
 * Identify an SPDX id from the body itself, by distinctive phrases. Only used
 * when the filename is uninformative — deliberately conservative, because a
 * wrong guess mislabels a reproduced licence.
 */
export function sniffSpdx(body: string): string | undefined {
  const t = body.replace(/\s+/g, ` `)
  if (t.includes(`Apache License`) && t.includes(`Version 2.0, January 2004`))
    return `Apache-2.0`
  if (t.includes(`Mozilla Public License Version 2.0`)) return `MPL-2.0`
  if (t.includes(`GNU GENERAL PUBLIC LICENSE`)) return `GPL`
  if (t.includes(`SIL OPEN FONT LICENSE Version 1.1`)) return `OFL-1.1`
  if (t.includes(`Boost Software License - Version 1.0`)) return `BSL-1.0`
  if (t.includes(`CC0 1.0 Universal`)) return `CC0-1.0`
  if (t.includes(`Eclipse Public License`)) return `EPL`
  if (
    t.includes(`Permission is hereby granted, free of charge`) &&
    t.includes(`THE SOFTWARE IS PROVIDED "AS IS"`)
  )
    return `MIT`
  if (t.includes(`Permission to use, copy, modify, and/or distribute`))
    return `ISC`
  if (t.includes(`Redistributions in binary form must reproduce`)) {
    return /neither the name|Neither the name/.test(body)
      ? `BSD-3-Clause`
      : `BSD-2-Clause`
  }
  if (t.includes(`This is free and unencumbered software released into`))
    return `Unlicense`
  return undefined
}

export interface FoundLicence {
  /** Filename inside the package directory. */
  file: string
  body: string
  /** Best guess at the SPDX id this body is, if any. */
  spdx?: string
}

/**
 * Every licence-shaped file in a package directory, sorted by filename so the
 * result never depends on the filesystem's readdir order.
 */
export function readLicenceFiles(dir: string): FoundLicence[] {
  if (!existsSync(dir)) return []
  let names: string[]
  try {
    names = readdirSync(dir)
  } catch {
    return []
  }
  const found: FoundLicence[] = []
  for (const name of names.sort(byCodepoint)) {
    const stem = name.replace(/\.(md|txt|rst)$/i, ``)
    if (!LICENCE_FILE.test(stem)) continue
    let raw: string
    try {
      raw = readFileSync(join(dir, name), `utf8`)
    } catch {
      continue
    }
    // A directory named LICENSE, or a stub pointing elsewhere.
    if (raw.trim().length < 40) continue
    const body = normaliseBody(raw)
    found.push({ file: name, body, spdx: spdxFromFilename(name) ?? sniffSpdx(body) })
  }
  return found
}

/**
 * Copyright lines carried by a body. These are printed per component even when
 * the body is shared, because the copyright notice — not the permission text —
 * is the part MIT/BSD/ISC actually require us to reproduce.
 */
export function extractCopyright(body: string): string[] {
  const lines: string[] = []
  for (const raw of body.split(`\n`)) {
    const line = raw.trim()
    if (!/^(copyright|\(c\)|©)/i.test(line)) continue
    // A real notice names a year. Requiring one is what keeps prose out of the
    // list: Apache-2.0's own body contains "copyright license to reproduce…"
    // and "(c) You must retain, in the Source form…", both of which start with
    // the trigger words and neither of which is anybody's copyright notice.
    if (!/\b(19|20)\d{2}\b/.test(line)) continue
    // Placeholders in SPDX templates and Apache-2.0's appendix.
    if (/<year>|<copyright|\[yyyy\]|\[name of copyright owner\]/i.test(line))
      continue
    if (line.length > 200) continue
    if (!lines.includes(line)) lines.push(line)
    if (lines.length >= 6) break
  }
  return lines
}

/** Load a canonical SPDX template body, or undefined when we have none. */
export function spdxTemplate(
  textsDir: string,
  spdx: string
): string | undefined {
  const file = join(textsDir, `spdx`, `${spdx}.txt`)
  if (!existsSync(file)) return undefined
  return normaliseBody(readFileSync(file, `utf8`))
}
