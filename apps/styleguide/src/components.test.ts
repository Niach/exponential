/**
 * The Components group's drift gates.
 *
 * A hand-written reference page rots two ways, and both are checked here:
 *
 *   1. the STATUS TABLE goes stale — a file is renamed and the table still
 *      points a reader at a path that no longer exists, or a platform silently
 *      drops out of the record
 *   2. the DEMOS drift off the tokens — someone pastes a hex value or a `24px`
 *      radius into the stylesheet and the specimen stops being generated from
 *      `@exp/design-tokens`, which is the only reason to trust it
 *
 * So: every `ok`/`leftover` file must exist on disk, every colour lives behind
 * a `:root` var declared from the tokens, and every radius is a ladder step.
 *
 * No disk store is read — `renderHtml` is pure, so an EMPTY `GalleryData`
 * literal exercises the whole component path without a `shots/` directory.
 */
import { describe, expect, test } from "bun:test"
import { existsSync } from "node:fs"
import { resolve } from "node:path"

import { GROUPS, VIEWS } from "@exp/view-catalog"

import { componentStyles } from "./component-styles.ts"
import { COMPONENTS, COMPONENTS_GROUP, COMPONENT_PLATFORMS } from "./components.ts"
import { renderHtml } from "./render.ts"
import { styles } from "./styles.ts"
import type { GalleryData } from "./store.ts"

const REPO_ROOT = resolve(import.meta.dir, `../../..`)

const EMPTY: GalleryData = {
  groups: [],
  views: [],
  undeclared: [],
  storeDir: resolve(REPO_ROOT, `shots`),
  indexPresent: false,
  counts: { ok: 0, missing: 0, manual: 0, na: 0 },
}

const html = renderHtml(EMPTY)

/** How often `needle` occurs in `haystack`. */
function occurrences(haystack: string, needle: string): number {
  let count = 0
  let at = haystack.indexOf(needle)
  while (at >= 0) {
    count += 1
    at = haystack.indexOf(needle, at + needle.length)
  }
  return count
}

/** Every class name any element in `markup` carries. */
function classNames(markup: string): string[] {
  const out: string[] = []
  for (const match of markup.matchAll(/class="([^"]*)"/g)) {
    for (const name of match[1]!.split(/\s+/)) if (name.length > 0) out.push(name)
  }
  return out
}

/** CSS with `/* … *\/` comments removed, so a commented-out value can't pass. */
function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, ``)
}

describe(`ids`, () => {
  test(`unique, kebab-case, and free of catalog collisions`, () => {
    const ids = COMPONENTS.map((spec) => spec.id)
    expect(new Set(ids).size).toBe(ids.length)
    const taken = new Set<string>([
      ...VIEWS.map((view) => view.id),
      ...GROUPS.map((group) => group.id),
      COMPONENTS_GROUP.id,
    ])
    for (const id of ids) {
      expect(id).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/)
      expect(taken.has(id)).toBe(false)
    }
  })
})

describe(`render`, () => {
  test(`one section and one nav link per component`, () => {
    for (const spec of COMPONENTS) {
      expect(occurrences(html, `<section class="view component" data-view="${spec.id}"`)).toBe(1)
      expect(
        occurrences(html, `<a class="nav-link" href="#${spec.id}" data-view="${spec.id}"`)
      ).toBe(1)
    }
  })

  test(`the group only appears when components are rendered`, () => {
    expect(html).toContain(`data-group="components"`)
    expect(renderHtml(EMPTY, [])).not.toContain(`data-group="components"`)
    expect(renderHtml(EMPTY, [])).not.toContain(`class="view component"`)
  })

  test(`inline JSON carries the components in order, and leaves the counts alone`, () => {
    const start = html.indexOf(`<script type="application/json" id="gallery-data">`)
    const from = html.indexOf(`>`, start) + 1
    const raw = html.slice(from, html.indexOf(`</script>`, from)).replace(/\\u003c/g, `<`)
    const parsed = JSON.parse(raw) as {
      components: { id: string; title: string }[]
      counts: GalleryData[`counts`]
      views: unknown[]
    }
    expect(parsed.components.map((entry) => entry.id)).toEqual(COMPONENTS.map((spec) => spec.id))
    expect(parsed.counts).toEqual(EMPTY.counts)
    expect(parsed.views).toEqual([])
  })

  test(`the summary counts components as its own part`, () => {
    expect(html).toContain(`${COMPONENTS.length} components`)
  })
})

describe(`demo markup`, () => {
  // Anything a demo needs beyond a `.cmp-*` block class. Kept SHORT on purpose:
  // a demo that wants a new structural name is usually a demo that should have
  // reused a block.
  const STRUCTURAL = new Set([
    `active`,
    `amount`,
    `bar`,
    `box`,
    `chevron`,
    `content`,
    `desc`,
    `destructive`,
    `disabled`,
    `divider`,
    `fill`,
    `glyph`,
    `grabber`,
    `header`,
    `interactive`,
    `item`,
    `label`,
    `line`,
    `name`,
    `on`,
    `step`,
    `tab`,
    `text`,
    `title`,
    `track`,
    `trailing`,
    `value`,
    `warn`,
  ])
  const TOKEN_MODIFIER = /^(fill|stroke|r|size|dur|ease)-[a-z0-9-]+$/

  test(`no inline styles, and only known class names`, () => {
    for (const spec of COMPONENTS) {
      const markup = spec.render()
      expect(markup).not.toContain(`style="`)
      for (const name of classNames(markup)) {
        const known =
          /^cmp-[a-z0-9-]+$/.test(name) || STRUCTURAL.has(name) || TOKEN_MODIFIER.test(name)
        expect(known ? name : `${spec.id}: unknown class "${name}"`).toBe(name)
      }
    }
  })
})

describe(`component stylesheet`, () => {
  const css = stripComments(componentStyles)

  test(`carries no colour literal`, () => {
    expect(css).not.toMatch(/oklch\(|rgba?\(|hsla?\(|#[0-9a-f]{3,8}\b/i)
  })

  test(`every var it uses is declared in the :root block`, () => {
    const root = stripComments(styles)
    const block = root.slice(root.indexOf(`:root {`), root.indexOf(`}`, root.indexOf(`:root {`)))
    const declared = new Set(
      [...block.matchAll(/^\s*(--[a-z0-9-]+)\s*:/gm)].map((match) => match[1]!)
    )
    const used = new Set([...css.matchAll(/var\((--[a-z0-9-]+)\)/g)].map((match) => match[1]!))
    expect(used.size).toBeGreaterThan(0)
    for (const name of used) {
      expect(declared.has(name) ? name : `${name} is not declared in :root`).toBe(name)
    }
  })

  test(`every radius is a ladder step, a capsule or a circle`, () => {
    const allowed = /^(var\(--r-(sm|md|lg|xl|xl2|xl3)\)|9999px|50%|0)$/
    for (const match of css.matchAll(/border-radius:\s*([^;}]+)/g)) {
      for (const token of match[1]!.trim().split(/[\s/]+/)) {
        expect(allowed.test(token) ? token : `bad radius "${token}"`).toBe(token)
      }
    }
  })
})

describe(`status table`, () => {
  test(`every platform is accounted for, and every named file exists`, () => {
    for (const spec of COMPONENTS) {
      for (const platform of COMPONENT_PLATFORMS) {
        const status = spec.status[platform]
        expect(status === undefined ? `${spec.id}/${platform} is missing` : platform).toBe(platform)
        if (status.state === `n/a`) {
          expect(status.note).toBeDefined()
          continue
        }
        expect(status.symbol).toBeDefined()
        expect(status.file).toBeDefined()
        const file = resolve(REPO_ROOT, status.file!)
        expect(existsSync(file) ? status.file : `${spec.id}/${platform}: ${status.file} is gone`).toBe(
          status.file
        )
      }
    }
  })

  test(`notes stay one short line`, () => {
    for (const spec of COMPONENTS) {
      for (const platform of COMPONENT_PLATFORMS) {
        const note = spec.status[platform].note
        if (note === undefined) continue
        expect(note.length).toBeLessThanOrEqual(120)
        expect(note).not.toContain(`\n`)
      }
    }
  })
})
