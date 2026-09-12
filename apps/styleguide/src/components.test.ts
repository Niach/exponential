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

/** The declarations of ONE rule, so a sibling rule's fill can't answer for it. */
function ruleBody(selector: string): string {
  const css = stripComments(componentStyles)
  const at = css.indexOf(`${selector} {`)
  return at < 0 ? `` : css.slice(at, css.indexOf(`}`, at))
}

/** The spec with that id — the demos are asserted through their markup. */
function spec(id: string): { blurb: string; markup: string } {
  const found = COMPONENTS.find((entry) => entry.id === id)
  expect(found === undefined ? `${id} is missing` : id).toBe(id)
  return { blurb: found?.blurb ?? ``, markup: found === undefined ? `` : found.render() }
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
    `add`,
    `amount`,
    `badge`,
    `bar`,
    `borderless`,
    `box`,
    `bubble`,
    `caption`,
    `card`,
    `card-head`,
    `chevron`,
    `close`,
    `content`,
    `desc`,
    `destructive`,
    `disabled`,
    `divider`,
    `dock`,
    `dot`,
    `field`,
    `fill`,
    `fold`,
    `glyph`,
    `grabber`,
    `header`,
    `id`,
    `image`,
    `interactive`,
    `item`,
    `label`,
    `line`,
    `name`,
    `narration`,
    `nav`,
    `on`,
    `opaque`,
    `panel`,
    `replies`,
    `reply`,
    `reply-body`,
    `reply-row`,
    `selected`,
    `show-more`,
    `step`,
    `strip`,
    `submit`,
    `tab`,
    `text`,
    `title`,
    `tool`,
    `tool-row`,
    `tools`,
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

describe(`the pill absorbed the chip and the header button`, () => {
  test(`neither survives as an id or a block class`, () => {
    const ids = new Set(COMPONENTS.map((spec) => spec.id))
    expect(ids.has(`chip`)).toBe(false)
    expect(ids.has(`button-xs`)).toBe(false)
    expect(componentStyles).not.toContain(`.cmp-chip`)
    expect(componentStyles).not.toContain(`.cmp-button-xs`)
  })

  test(`the demo shows all six size × mode combinations`, () => {
    const spec = COMPONENTS.find((entry) => entry.id === `pill`)
    const markup = spec === undefined ? `` : spec.render()
    for (const pillSize of [`md`, `sm`]) {
      for (const mode of [`action`, `select`, `readonly`]) {
        const combination = `data-size="${pillSize}" data-mode="${mode}"`
        expect(markup.includes(combination) ? combination : `pill: ${combination} is missing`).toBe(
          combination
        )
      }
    }
  })
})

describe(`a circle is the primary action, a rounded square is a picker (EXP-771/EXP-862)`, () => {
  test(`the icon button stays a circle, the picker takes the MD step`, () => {
    expect(ruleBody(`.cmp-icon-button`)).toContain(`border-radius: 50%`)
    expect(ruleBody(`.cmp-icon-picker-trigger`)).toContain(`border-radius: var(--r-md)`)
    expect(ruleBody(`.cmp-icon-grid .item`)).toContain(`border-radius: var(--r-md)`)
  })

  test(`the ghost has no circle, no fill and no stroke, only a hover wash`, () => {
    const ghost = ruleBody(`.cmp-ghost-icon-button`)
    expect(ghost).toContain(`border: none`)
    expect(ghost).toContain(`background: transparent`)
    expect(ghost).toContain(`border-radius: var(--r-md)`)
    expect(ghost).not.toContain(`50%`)
    expect(ghost).not.toContain(`1px solid`)
    expect(ruleBody(`.cmp-ghost-icon-button:hover`)).toContain(`background: var(--active)`)
  })

  test(`the ghost demo is the secondary set, the circle demo the primary one`, () => {
    const ghost = spec(`ghost-icon-button`)
    expect(occurrences(ghost.markup, `class="cmp-ghost-icon-button"`)).toBe(3)
    expect(ghost.markup).not.toContain(`class="cmp-icon-button"`)
    expect(ghost.blurb).toContain(`SECONDARY`)
    const circle = spec(`icon-button`)
    expect(circle.markup).not.toContain(`class="cmp-ghost-icon-button"`)
    expect(occurrences(circle.markup, `class="cmp-icon-button"`)).toBe(3)
    expect(circle.blurb).toContain(`ghost icon button`)
  })

  test(`the demo shows both trigger states, the grid and a circle beside them`, () => {
    const { markup } = spec(`icon-picker`)
    expect(occurrences(markup, `class="cmp-icon-picker-trigger"`)).toBe(2)
    expect(occurrences(markup, `class="cmp-icon-picker-trigger" data-empty`)).toBe(1)
    expect(occurrences(markup, `class="item selected"`)).toBe(1)
    expect(markup).toContain(`class="cmp-icon-grid"`)
    expect(markup).toContain(`class="cmp-icon-button"`)
  })

  test(`the icon button and the radius ladder both name the exception`, () => {
    expect(spec(`icon-button`).blurb).toContain(`icon picker`)
    expect(spec(`tokens-radius`).blurb).toContain(`PICKER corner`)
  })
})

describe(`the bottom band sits on the ground, not in the card (EXP-771)`, () => {
  test(`the band draws no fill and no border`, () => {
    const band = ruleBody(`.cmp-session-bar`)
    expect(band).toContain(`height: 36px`)
    expect(band).toContain(`padding: 0 8px`)
    expect(band).not.toContain(`background`)
    expect(band).not.toContain(`border`)
  })

  test(`the demo hangs it under a card that stops 6px short`, () => {
    const { markup } = spec(`session-bar`)
    expect(markup).toContain(`class="cmp-session-ground"`)
    expect(markup.indexOf(`class="card"`)).toBeLessThan(markup.indexOf(`class="cmp-session-bar"`))
    expect(ruleBody(`.cmp-session-ground .card`)).toContain(`margin-bottom: 6px`)
  })

  test(`the shell owns both bands and the panel owns neither`, () => {
    const css = stripComments(componentStyles)
    expect(css).toContain(`.cmp-app-shell > .header`)
    expect(css).toContain(`.cmp-app-shell > .dock`)
    expect(css).not.toContain(`.cmp-app-shell .panel .dock`)
    expect(css).not.toContain(`.cmp-app-shell .panel .header`)
    const { markup } = spec(`app-shell`)
    expect(markup.indexOf(`class="header"`)).toBeLessThan(markup.indexOf(`class="panel"`))
    expect(markup.indexOf(`class="panel"`)).toBeLessThan(markup.indexOf(`class="dock"`))
  })
})

describe(`the settings page header`, () => {
  test(`title, subtitle and divider ride a centred 896 column`, () => {
    const { markup, blurb } = spec(`page-header`)
    expect(markup).toContain(`<div class="title">Settings</div>`)
    expect(markup).toMatch(/<div class="desc">Manage .+ and your account<\/div>/)
    expect(markup).toContain(`class="cmp-divider"`)
    expect(blurb).toContain(`Helpdesk`)
    const column = ruleBody(`.cmp-page-header .content`)
    expect(column).toContain(`max-width: 896px`)
    expect(column).toContain(`margin: 0 auto`)
    expect(column).toContain(`padding: 24px`)
    // The scroll region is the PANE, so the scrollbar rides the viewport edge.
    expect(ruleBody(`.cmp-page-header`)).toContain(`overflow-y: auto`)
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
