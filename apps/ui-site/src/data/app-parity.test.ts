import { existsSync } from "node:fs"
import { resolve } from "node:path"

import { expect, test } from "bun:test"

import catalog from "../../../../packages/exponential-ui/docs/components.generated.json"
import { APP_PARITY, appParityFor, type ParityPlatform } from "./app-parity"

// VAPP-93: the parity tables that moved off apps/styleguide stay honest.

const ROOT = resolve(import.meta.dir, `../../../..`)
const PLATFORMS: readonly ParityPlatform[] = [`web`, `desktop`, `ios`, `android`]
const NAMES = new Set(catalog.components.map((component) => component.name))

test(`ids are unique and every row maps onto at least one component`, () => {
  const ids = APP_PARITY.map((row) => row.id)
  expect(new Set(ids).size).toBe(ids.length)
  for (const row of APP_PARITY) expect(row.components.length).toBeGreaterThan(0)
})

test(`every component name is in the core catalog`, () => {
  for (const row of APP_PARITY) {
    for (const name of row.components) {
      expect(NAMES.has(name) ? name : `${row.id}: unknown component ${name}`).toBe(name)
    }
  }
})

test(`every named file exists relative to the repo root`, () => {
  for (const row of APP_PARITY) {
    const files = [
      ...PLATFORMS.map((platform) => row.status[platform].file),
      ...(row.leftovers ?? []).map((leftover) => leftover.file),
    ].filter((file): file is string => file !== undefined)
    for (const file of files) {
      expect(existsSync(resolve(ROOT, file)) ? file : `${row.id}: missing ${file}`).toBe(file)
    }
  }
})

test(`every platform is accounted for, and an ok/leftover cell names its file`, () => {
  for (const row of APP_PARITY) {
    for (const platform of PLATFORMS) {
      const cell = row.status[platform]
      expect([`ok`, `leftover`, `n/a`]).toContain(cell.status)
      if (cell.status !== `n/a`) expect(cell.file ? row.id : `${row.id}/${platform}: no file`).toBe(row.id)
    }
  }
})

test(`notes stay one line of at most 160 characters`, () => {
  const notes = APP_PARITY.flatMap((row) => [
    ...PLATFORMS.map((platform) => row.status[platform].note),
    ...(row.leftovers ?? []).map((leftover) => leftover.note),
  ]).filter((note): note is string => note !== undefined)
  for (const note of notes) {
    expect(note.includes(`\n`) ? `multi-line: ${note}` : note).toBe(note)
    expect(note.length <= 160 ? note : `too long (${note.length}): ${note}`).toBe(note)
  }
})

test(`appParityFor finds every row by each of its components`, () => {
  for (const row of APP_PARITY) {
    for (const name of row.components) expect(appParityFor(name)).toContain(row)
  }
  expect(appParityFor(`NoSuchComponent`)).toEqual([])
})
