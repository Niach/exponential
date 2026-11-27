// #EXP-2168 (fixtures/header-actions.json `guard`): ONE header concept on
// every page ×5. Every client's sources are scanned on every PR (the web
// shards always run): no header host's call holds a `banned` control (a
// ghost glyph, a hand-rolled circle, a Material IconButton): every control
// of a header is THE round button (`round`); the old header buttons (`gone`)
// exist nowhere. The shared component files (`shared`) are where the round
// button and its host are defined, and are exempt.

import { describe, expect, it } from "vitest"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"
import fixture from "@exp/domain-contract/fixtures/header-actions.json" with { type: "json" }

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)

type Guard = {
  roots: string[]
  ext: string
  round: string
  hosts: string[]
  banned: string[]
  gone: string[]
  shared: string[]
}

const GUARD = fixture.guard as Record<string, Guard>

function files(dir: string, ext: string): string[] {
  const out: string[] = []
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) {
      if (name !== `node_modules` && name !== `build` && name !== `.build`) out.push(...files(path, ext))
    } else if (name.endsWith(ext) && !/\.test\.|\.spec\.|Test\.kt$|Tests\.swift$/.test(name)) {
      out.push(path)
    }
  }
  return out
}

const OPEN: Record<string, string> = { "(": `)`, "{": `}`, "[": `]` }

/** The balanced block starting at `start` (an opening bracket). */
function block(src: string, start: number): number {
  const stack: string[] = []
  for (let i = start; i < src.length; i++) {
    const c = src[i]
    if (c in OPEN) stack.push(OPEN[c])
    else if (c === stack[stack.length - 1]) {
      stack.pop()
      if (stack.length === 0) return i + 1
    }
  }
  return src.length
}

/** A call's whole text from `at`: its argument list and trailing closures
 *  (Swift `trailing: { … }` labels too). */
function callText(src: string, at: number): string {
  let i = at
  let end = at
  for (;;) {
    while (i < src.length && /\s/.test(src[i])) i++
    if (src[i] === `(` || src[i] === `{`) {
      end = block(src, i)
      i = end
      continue
    }
    const label = /^[A-Za-z_]\w*:\s*\{/.exec(src.slice(i, i + 64))
    if (label && end > at) {
      i += label[0].length - 1
      continue
    }
    return src.slice(at, end)
  }
}

/** JSX: the opening tag `<Name …>` with its attribute expressions. */
function jsxTag(src: string, at: number): string {
  let depth = 0
  for (let i = at; i < src.length; i++) {
    const c = src[i]
    if (c === `{`) depth++
    else if (c === `}`) depth--
    else if (c === `>` && depth === 0) return src.slice(at, i + 1)
  }
  return src.slice(at)
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)
const word = (name: string) => new RegExp(`(?<![\\w])${escape(name)}(?![\\w])`)

/** Every call of `host` in `src` (definitions skipped), as its text. */
function hostCalls(src: string, host: string): { line: number; text: string }[] {
  const out: { line: number; text: string }[] = []
  const jsx = host.startsWith(`<`)
  const re = jsx ? new RegExp(`${escape(host)}(?=[\\s/>])`, `g`) : new RegExp(`(?<![\\w])${escape(host)}(?=\\s*[({])`, `g`)
  for (let m = re.exec(src); m; m = re.exec(src)) {
    const lineStart = src.lastIndexOf(`\n`, m.index) + 1
    const before = src.slice(lineStart, m.index)
    if (!jsx && /\b(fun|func|fn|struct|class|impl|enum|interface|function)\s+$/.test(before)) continue
    const text = jsx ? jsxTag(src, m.index) : callText(src, m.index + host.length)
    out.push({ line: src.slice(0, m.index).split(`\n`).length, text })
  }
  return out
}

describe(`one header concept on every page (#EXP-2168)`, () => {
  it(`guards every client`, () => {
    expect(Object.keys(GUARD).sort()).toEqual([`android`, `desktop`, `ios`, `web`])
  })

  for (const [client, guard] of Object.entries(GUARD)) {
    const sources = guard.roots.flatMap((root) => files(join(repoRoot, root), guard.ext))
    const shared = new Set(guard.shared)
    const scanned = sources
      .map((path) => ({ path: relative(repoRoot, path), src: readFileSync(path, `utf8`) }))
      .filter((file) => !shared.has(file.path))

    it(`${client}: the round button exists`, () => {
      expect(sources.some((path) => word(guard.round).test(readFileSync(path, `utf8`)))).toBe(true)
    })

    it(`${client}: the old header buttons are gone`, () => {
      const found = scanned.flatMap((file) =>
        guard.gone.filter((name) => word(name).test(file.src)).map((name) => `${file.path}: ${name}`)
      )
      expect(found).toEqual([])
    })

    it(`${client}: a header holds only THE round button`, () => {
      const found: string[] = []
      for (const file of scanned) {
        for (const host of guard.hosts) {
          for (const call of hostCalls(file.src, host)) {
            for (const name of guard.banned) {
              const hit = name.includes(`"`) || name.startsWith(`<`) ? call.text.includes(name) : word(name).test(call.text)
              if (hit) found.push(`${file.path}:${call.line} ${host} holds ${name}`)
            }
          }
        }
      }
      expect(found).toEqual([])
    })
  }
})
