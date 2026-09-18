/**
 * The Components group's drift gates.
 *
 * A reference page rots three ways, and all three are checked here:
 *
 *   1. the STATUS TABLE goes stale — a file is renamed and the table still
 *      points a reader at a path that no longer exists, or a platform silently
 *      drops out of the record
 *   2. an entry that COULD be the real component stays a lookalike — since
 *      EXP-887 every LISTED entry whose web symbol lives in `packages/ui`
 *      must render as an ISLAND, and nothing else may (the set of entries is
 *      curated; a new package module is not auto-listed)
 *   3. the hand-written demos drift off the tokens — someone pastes a hex
 *      value or a `24px` radius into the stylesheet and the specimen stops
 *      being generated from `@exp/design-tokens`, which is the only reason to
 *      trust it
 *
 * So: every `ok`/`leftover` file must exist on disk, the island split matches
 * the `packages/ui` split, every colour lives behind a `:root` var declared
 * from the tokens, and every radius is a ladder step.
 *
 * No disk store is read — `renderHtml` is pure, so an EMPTY `GalleryData`
 * literal exercises the whole component path without a `shots/` directory. The
 * `@exp/ui` stylesheet IS compiled, exactly ONCE for the whole suite: it is the
 * slow part (~1s) and every island assertion shares it.
 */
import { describe, expect, test } from "bun:test"
import { existsSync, readFileSync } from "node:fs"
import { resolve } from "node:path"

import { contract } from "@exp/domain-contract"
import { formatDateLabel } from "@exp/ui"
import { GROUPS, VIEWS } from "@exp/view-catalog"
import {
  ISLAND_CLIENT_SCRIPT,
  ISLAND_ROOT_CLASS,
  compileUiCss,
  renderIsland,
} from "@exp/ui/island"

import { componentStyles } from "./component-styles.ts"
import {
  COMPONENTS,
  COMPONENTS_GROUP,
  COMPONENT_PLATFORMS,
  KIND_ORDER,
  MODES,
  PORTAL_ONLY_IDS,
  STYLE_KINDS,
  isIsland,
  modeOf,
} from "./components.tsx"
import type { ComponentSpec, Mode } from "./components.tsx"
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

const uiCss = await compileUiCss({ base: import.meta.dir })
const html = renderHtml(EMPTY, COMPONENTS, uiCss)

const ISLANDS = COMPONENTS.filter(isIsland)
const HTML_DEMOS = COMPONENTS.filter((spec) => !isIsland(spec))

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

/** Whatever the page puts inside that entry's `.cmp-demo` canvas. */
function demoMarkup(spec: ComponentSpec): string {
  return isIsland(spec) ? renderIsland(spec.island()) : spec.render()
}

/** The spec with that id — the demos are asserted through their markup. */
function spec(id: string): { blurb: string; markup: string } {
  const found = COMPONENTS.find((entry) => entry.id === id)
  expect(found === undefined ? `${id} is missing` : id).toBe(id)
  return {
    blurb: found?.blurb ?? ``,
    markup: found === undefined ? `` : demoMarkup(found),
  }
}

/** What an island actually rendered: its shadow root's single wrapper child. */
function islandBody(id: string): string {
  const markup = spec(id).markup
  const open = `<div class="${ISLAND_ROOT_CLASS}">`
  const at = markup.indexOf(open)
  expect(at < 0 ? `${id} is not an island` : id).toBe(id)
  return markup.slice(at + open.length, markup.lastIndexOf(`</div></template>`))
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
      expect(
        occurrences(
          html,
          `<section class="view component" data-mode="${modeOf(spec)}" data-view="${spec.id}"`
        )
      ).toBe(1)
      expect(
        occurrences(html, `<a class="nav-link" href="#${spec.id}" data-view="${spec.id}"`)
      ).toBe(1)
    }
  })

  test(`the synthetic modes only appear when components are rendered`, () => {
    // EXP-941 banded the two synthetic modes by `kind`, so the group id a
    // reader can point at is the MODE, not one appended catalog group.
    expect(html).toContain(`<div class="mode-section" data-mode="components">`)
    expect(html).toContain(`<div class="mode-section" data-mode="style">`)
    const bare = renderHtml(EMPTY, [])
    expect(bare).toContain(`<div class="mode-section" data-mode="components"></div>`)
    expect(bare).not.toContain(`class="view component"`)
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

  test(`the summary counts each synthetic mode as its own part`, () => {
    const components = COMPONENTS.filter((spec) => modeOf(spec) === `components`)
    const style = COMPONENTS.filter((spec) => modeOf(spec) === `style`)
    expect(html).toContain(`${components.length} components · ${style.length} style`)
    expect(components.length + style.length).toBe(COMPONENTS.length)
  })
})

describe(`islands (EXP-887)`, () => {
  test(`most of the group is the real component, not a lookalike`, () => {
    expect(ISLANDS.length).toBeGreaterThan(HTML_DEMOS.length / 2)
  })

  test(`every island renders markup, inside a shadow root, on the app's face`, () => {
    for (const entry of ISLANDS) {
      const markup = renderIsland(entry.island())
      expect(markup.startsWith(`<div data-ui-island><template shadowrootmode="open">`)).toBe(true)
      expect(markup).toContain(`<div class="${ISLAND_ROOT_CLASS}">`)
      const body = islandBody(entry.id)
      // Not merely non-empty: a Radix portal renders "" and an empty wrapper
      // would sail through a length check on the island as a whole.
      expect(body.length > 40 ? entry.id : `${entry.id}: island rendered "${body}"`).toBe(entry.id)
      expect(body.startsWith(`<`) ? entry.id : `${entry.id}: island is not an element`).toBe(
        entry.id
      )
    }
  })

  test(`the page carries one shadow root per island, the CSS once, the script once`, () => {
    expect(occurrences(html, `<template shadowrootmode="open">`)).toBe(ISLANDS.length)
    expect(occurrences(html, `<div data-ui-island>`)).toBe(ISLANDS.length)
    expect(occurrences(html, `<template id="ui-css">`)).toBe(1)
    expect(occurrences(html, ISLAND_CLIENT_SCRIPT)).toBe(1)
    // Both are gated on the stylesheet: no CSS, no islands worth adopting.
    const bare = renderHtml(EMPTY, COMPONENTS)
    expect(bare).not.toContain(`<template id="ui-css">`)
    expect(bare).not.toContain(ISLAND_CLIENT_SCRIPT)
    expect(occurrences(bare, `<template shadowrootmode="open">`)).toBe(ISLANDS.length)
  })

  test(`every island sits in the same .cmp-demo canvas the HTML demos use`, () => {
    for (const entry of ISLANDS) {
      expect(html).toContain(`<div class="cmp-demo"><div data-ui-island>`)
      expect(html).toContain(`id="view-${entry.id}"`)
    }
    // The ground is the CANVAS's, never the island's: a fixed gradient layer
    // inside a shadow tree is wrong (see @exp/ui/island).
    expect(ISLAND_ROOT_CLASS).not.toContain(`bg-`)
  })

  test(`the compiled stylesheet carries the theme, the chip box and the palette`, () => {
    // The chip's box lives in the package stylesheet, not in a utility…
    expect(uiCss).toContain(`.issue-chip{`)
    // …the theme vars reach a shadow tree through `:host`…
    expect(uiCss).toMatch(/:root,\s?:host/)
    expect(uiCss).toContain(`--color-glass-card`)
    // …and a palette utility only a component file names is still emitted,
    // which is the whole reason the package's own `@source` exists.
    expect(uiCss).toContain(`.text-yellow-500`)
    expect(uiCss).not.toContain(`</style`)
  })

  test(`the page mirrors the package's root font size, or islands render at the wrong scale`, () => {
    const packageCss = readFileSync(
      resolve(REPO_ROOT, `packages/ui/src/styles.css`),
      `utf8`
    )
    const rootSize = /html\s*\{\s*font-size:\s*([^;]+);/
    const mobileSize = /@media\s*\(max-width:\s*767px\)\s*\{\s*html\s*\{\s*font-size:\s*([^;]+);/
    const theirs = packageCss.match(rootSize)?.[1]
    const theirsMobile = packageCss.match(mobileSize)?.[1]
    expect(theirs).toBeDefined()
    expect(theirsMobile).toBeDefined()
    expect(styles.match(rootSize)?.[1]).toBe(theirs!)
    expect(styles.match(mobileSize)?.[1]).toBe(theirsMobile!)
  })
})

describe(`demo markup`, () => {
  // Anything a HAND-WRITTEN demo needs beyond a `.cmp-*` block class. Kept
  // SHORT on purpose: a demo that wants a new structural name is usually a
  // demo that should have reused a block — or, since EXP-887, one that should
  // have been an island.
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
    `footer`,
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
    // Islands are exempt by construction: they wear Tailwind utilities and
    // whatever inline style the real component sets (an avatar's hue).
    for (const spec of HTML_DEMOS) {
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

  test(`the demo shows all six size × mode combinations, read off the real Pill`, () => {
    const markup = islandBody(`pill`)
    const seen = new Set<string>()
    for (const tag of markup.matchAll(/<(?:span|button)[^>]*data-slot="pill"[^>]*>/g)) {
      const size = tag[0].match(/data-size="([^"]+)"/)?.[1]
      const mode = tag[0].match(/data-mode="([^"]+)"/)?.[1]
      expect(size === undefined ? `a pill rendered no data-size` : `ok`).toBe(`ok`)
      expect(mode === undefined ? `a pill rendered no data-mode` : `ok`).toBe(`ok`)
      seen.add(`${size}/${mode}`)
    }
    for (const pillSize of [`md`, `sm`]) {
      for (const mode of [`action`, `select`, `readonly`]) {
        const combination = `${pillSize}/${mode}`
        expect(seen.has(combination) ? combination : `pill: ${combination} is missing`).toBe(
          combination
        )
      }
    }
  })
})

describe(`a circle is the primary action, a rounded square is a picker (EXP-771/EXP-862)`, () => {
  test(`the hand-written icon button stays a circle, the ghost keeps only a hover wash`, () => {
    // Both blocks survive because the COMPOSITION demos still use them (a
    // relation row's remove, a GitHub account row's ✕).
    expect(ruleBody(`.cmp-icon-button`)).toContain(`border-radius: 50%`)
    const ghost = ruleBody(`.cmp-ghost-icon-button`)
    expect(ghost).toContain(`border: none`)
    expect(ghost).toContain(`background: transparent`)
    expect(ghost).toContain(`border-radius: var(--r-md)`)
    expect(ghost).not.toContain(`50%`)
    expect(ghost).not.toContain(`1px solid`)
    expect(ruleBody(`.cmp-ghost-icon-button:hover`)).toContain(`background: var(--active)`)
  })

  test(`the real buttons carry the shape rule: glass circle vs ghost MD corner`, () => {
    const circle = islandBody(`icon-button`)
    expect(occurrences(circle, `data-variant="glass" data-size="icon-sm"`)).toBe(3)
    expect(circle).not.toContain(`data-variant="ghost"`)
    expect(circle).toContain(`rounded-full`)
    expect(circle).not.toContain(`rounded-md`)

    const ghost = islandBody(`ghost-icon-button`)
    expect(occurrences(ghost, `data-variant="ghost" data-size="icon-sm"`)).toBe(3)
    expect(ghost).not.toContain(`data-variant="glass"`)
    expect(ghost).toContain(`rounded-md`)

    expect(spec(`ghost-icon-button`).blurb).toContain(`SECONDARY`)
    expect(spec(`icon-button`).blurb).toContain(`ghost icon button`)
  })

  test(`the picker demo shows both trigger states, the grid and a circle beside them`, () => {
    const markup = islandBody(`icon-picker`)
    // Two triggers, exactly one of them empty (dashed hairline + placeholder).
    expect(occurrences(markup, `aria-label="Pick an icon"`)).toBe(1)
    expect(occurrences(markup, `border-dashed`)).toBe(1)
    expect(occurrences(markup, `aria-label="Icon: flag"`)).toBe(1)
    // The trigger and every grid cell take the MD step, never the circle.
    expect(occurrences(markup, `rounded-md`)).toBeGreaterThan(10)
    expect(occurrences(markup, `aria-pressed="true"`)).toBe(1)
    // The primary circle beside them is the counter-example.
    expect(markup).toContain(`data-variant="glass" data-size="icon-sm"`)
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
    // EXP-887 retired `--r-chip`: the issue chip is an island now and wears
    // the package's own `.issue-chip` box, so this page needs no sub-ladder
    // corner of its own.
    const allowed = /^(var\(--r-(sm|md|lg|xl|xl2|xl3)\)|9999px|50%|0)$/
    for (const match of css.matchAll(/border-radius:\s*([^;}]+)/g)) {
      for (const token of match[1]!.trim().split(/[\s/]+/)) {
        expect(allowed.test(token) ? token : `bad radius "${token}"`).toBe(token)
      }
    }
    expect(styles).not.toContain(`--r-chip`)
  })

  test(`no rule left behind: every .cmp- block a demo no longer uses is gone`, () => {
    const declared = new Set(
      [...css.matchAll(/\.(cmp-[a-z0-9-]+)/g)].map((match) => match[1]!)
    )
    const used = new Set<string>()
    for (const spec of HTML_DEMOS) {
      for (const name of classNames(spec.render())) {
        if (name.startsWith(`cmp-`)) used.add(name)
      }
    }
    // `.cmp-demo` is the renderer's canvas, not a demo's own class.
    used.add(`cmp-demo`)
    for (const name of declared) {
      expect(used.has(name) ? name : `.${name} is declared but no demo uses it`).toBe(name)
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

  test(`a web symbol in @exp/ui is an island, and nothing else is`, () => {
    for (const spec of COMPONENTS) {
      const file = spec.status.web.file
      const inPackage = file !== undefined && file.startsWith(`packages/ui/`) && file.endsWith(`.tsx`)
      // A STYLE entry documents a VALUE, not a control: it names whichever file
      // happens to spend the token, so it is normally a hand-written swatch
      // table. EXP-941 relaxed that ONE way — a Style entry whose web file
      // lives under `packages/ui/` MAY be an island, because the registry
      // (`tokens-icons`) is best documented by rendering its own glyphs, and
      // its source is a `.ts`, not a component file.
      const styleUnderPackage =
        modeOf(spec) === `style` && file !== undefined && file.startsWith(`packages/ui/`)
      if (styleUnderPackage) continue
      const portalOnly = PORTAL_ONLY_IDS.includes(spec.id)
      const expected = inPackage && !portalOnly
      const verdict =
        isIsland(spec) === expected
          ? spec.id
          : expected
            ? `${spec.id}: ${file} is in @exp/ui — render the real component as an island`
            : `${spec.id}: only a @exp/ui component may be an island (web is ${file ?? `n/a`})`
      expect(verdict).toBe(spec.id)
    }
  })

  test(`the portal exception stays honest`, () => {
    for (const id of PORTAL_ONLY_IDS) {
      const found = COMPONENTS.find((spec) => spec.id === id)
      expect(found === undefined ? `${id} is not a component` : id).toBe(id)
      // It is only an EXCEPTION while the symbol really did move into the
      // package; otherwise it is an ordinary hand-written composition and the
      // list should shrink.
      expect(found?.status.web.file?.startsWith(`packages/ui/`)).toBe(true)
      expect(found !== undefined && isIsland(found)).toBe(false)
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

describe(`the GitHub connect surfaces (FEED-42)`, () => {
  test(`the connection block renders every state line of the installed form`, () => {
    const { markup, blurb } = spec(`github-connection`)
    for (const text of [
      `Repositories`,
      `Add repository`,
      `GitHub accounts connected to this team`,
      `acme`,
      `octocat`,
      `Configure`,
      `An installation is per GitHub account or organization.`,
      `Connect another account`,
      `Refresh access`,
      `Reconnect`,
      `covers installation 42 anymore`,
      `Disconnect account`,
      `No GitHub account connected`,
      `Connect GitHub`,
      `Install on an account`,
    ]) {
      expect(markup.includes(text) ? text : `github-connection: "${text}" is missing`).toBe(text)
    }
    // The ✕ is a ghost, one per account, never a primary circle.
    expect(occurrences(markup, `class="cmp-ghost-icon-button"`)).toBe(2)
    expect(markup).not.toContain(`class="cmp-icon-button"`)
    expect(blurb).toContain(`integrations.github.status`)
  })

  test(`the picker renders the banner, rows and the dashed footer with the lookup`, () => {
    const { markup } = spec(`repo-picker`)
    for (const text of [
      `Reconnect GitHub (octocat) to refresh.`,
      `Search repositories…`,
      `Only repositories your GitHub installation grants appear here.`,
      `Showing the first 500 repositories per account`,
      `Refresh`,
      `Install on another account`,
      `owner/name`,
      `Look up`,
      `Repository not found, or no connected installation grants it.`,
    ]) {
      expect(markup.includes(text) ? text : `repo-picker: "${text}" is missing`).toBe(text)
    }
    expect(occurrences(markup, `class="cmp-repo-picker-row"`)).toBe(3)
    expect(ruleBody(`.cmp-repo-picker-footer`)).toContain(`dashed var(--stroke-strong)`)
  })
})

describe(`modes (EXP-941)`, () => {
  test(`every spec lands in exactly one mode, and Style holds only Style kinds`, () => {
    const components = COMPONENTS.filter((spec) => modeOf(spec) === `components`)
    const style = COMPONENTS.filter((spec) => modeOf(spec) === `style`)
    expect(components.length + style.length).toBe(COMPONENTS.length)
    expect(components.length).toBeGreaterThan(0)
    expect(style.length).toBeGreaterThan(0)
    for (const spec of style) {
      expect(STYLE_KINDS.includes(spec.kind) ? spec.id : `${spec.id}: ${spec.kind} is not a Style kind`).toBe(spec.id)
    }
    for (const spec of components) {
      expect(STYLE_KINDS.includes(spec.kind) ? `${spec.id}: ${spec.kind} belongs to Style` : spec.id).toBe(spec.id)
    }
    // Every kind a spec carries has a place in its mode's nav order, or the
    // entry would render in no band at all.
    for (const spec of COMPONENTS) {
      const order = KIND_ORDER[modeOf(spec)]
      expect(order.includes(spec.kind) ? spec.id : `${spec.id}: ${spec.kind} is not in KIND_ORDER`).toBe(spec.id)
    }
  })

  test(`three mode sections, three segments, and the counts are the link counts`, () => {
    const modes = Object.keys(MODES) as Mode[]
    expect(modes).toEqual([`views`, `components`, `style`])
    for (const mode of modes) {
      expect(occurrences(html, `<div class="mode-section" data-mode="${mode}">`)).toBe(1)
      const open = html.indexOf(`<div class="mode-section" data-mode="${mode}">`)
      const section = html.slice(open, html.indexOf(`<div class="mode-section"`, open + 1) >= 0 ? html.indexOf(`<div class="mode-section"`, open + 1) : html.indexOf(`<div class="nav-empty`, open))
      const links = occurrences(section, `<a class="nav-link" href="#`)
      const button = html.slice(html.indexOf(`<button class="mode-btn" type="button" data-mode="${mode}"`))
      const count = Number(button.slice(button.indexOf(`<span class="count">`) + 20, button.indexOf(`</span></button>`)))
      expect(`${mode}:${count}`).toBe(`${mode}:${links}`)
    }
    // The empty gallery still renders all three: a mode bar that appears and
    // disappears is a mode bar nobody learns.
    expect(occurrences(html, `<button class="mode-btn"`)).toBe(3)
  })

  test(`every .view carries the data-mode of its nav link`, () => {
    const linkMode = new Map<string, string>()
    for (const match of html.matchAll(/<div class="mode-section" data-mode="([a-z]+)">([\s\S]*?)(?=<div class="mode-section"|<div class="nav-empty)/g)) {
      for (const link of match[2]!.matchAll(/<a class="nav-link" href="#([a-z0-9-]+)"/g)) {
        linkMode.set(link[1]!, match[1]!)
      }
    }
    expect(linkMode.size).toBe(COMPONENTS.length)
    for (const section of html.matchAll(/<section class="view[^"]*" data-mode="([a-z]+)" data-view="([a-z0-9-]+)"/g)) {
      const mode = linkMode.get(section[2]!)
      expect(mode === undefined ? `${section[2]}: no nav link` : `${section[2]}:${section[1]}`).toBe(
        `${section[2]}:${mode}`
      )
    }
  })

  test(`the mode bar is painted from the tokens, like everything else`, () => {
    const css = stripComments(styles)
    const at = css.indexOf(`.mode-bar {`)
    expect(at).toBeGreaterThan(0)
    const rule = css.slice(at, css.indexOf(`}`, at))
    expect(rule).not.toMatch(/oklch\(|rgba?\(|hsla?\(|#[0-9a-f]{3,8}\b/i)
    // The segmented capsule from the tokens: section fill under section stroke.
    expect(rule).toContain(`background: var(--section)`)
    expect(rule).toContain(`border: 1px solid var(--stroke-section)`)
    expect(rule).toContain(`height: 36px`)
    expect(rule).toContain(`padding: 3px`)
    expect(rule).toContain(`border-radius: 9999px`)
    const active = css.slice(css.indexOf(`.mode-btn[aria-pressed="true"] {`))
    expect(active.slice(0, active.indexOf(`}`))).toContain(`background: var(--active)`)
  })

  test(`the size toggle and the shot hint only exist in Views`, () => {
    expect(html).toContain(`<button id="toggle-size" class="btn views-only"`)
    expect(stripComments(styles)).toContain(`body:not([data-mode="views"]) .views-only { display: none; }`)
  })
})

describe(`leftovers (EXP-941)`, () => {
  const WITH_LEFTOVERS = COMPONENTS.filter((spec) => (spec.leftovers ?? []).length > 0)

  test(`the page names call sites the product still draws by hand`, () => {
    expect(WITH_LEFTOVERS.length).toBeGreaterThan(0)
    for (const spec of WITH_LEFTOVERS) {
      expect(html).toContain(`<div class="leftovers"><div class="leftovers-head">Still drawn by hand</div>`)
      for (const row of spec.leftovers!) expect(html).toContain(row.file)
    }
    // An entry with none renders no block at all, so silence stays silence.
    const clean = COMPONENTS.filter((spec) => (spec.leftovers ?? []).length === 0)
    expect(clean.length).toBeGreaterThan(0)
    expect(occurrences(html, `<div class="leftovers">`)).toBe(WITH_LEFTOVERS.length)
  })

  test(`every named file exists, and the notes stay one short line`, () => {
    for (const spec of WITH_LEFTOVERS) {
      for (const row of spec.leftovers!) {
        const file = resolve(REPO_ROOT, row.file)
        expect(existsSync(file) ? row.file : `${spec.id}: ${row.file} is gone`).toBe(row.file)
        expect(row.note.length).toBeLessThanOrEqual(120)
        expect(row.note).not.toContain(`\n`)
        expect(row.note.length > 0 ? spec.id : `${spec.id}: an empty leftover note`).toBe(spec.id)
      }
    }
  })

  test(`a leftover adds ONE extra dot to the nav link, and nothing else does`, () => {
    for (const spec of COMPONENTS) {
      const at = html.indexOf(`<a class="nav-link" href="#${spec.id}" data-view="${spec.id}"`)
      const link = html.slice(at, html.indexOf(`</a>`, at))
      const expected = COMPONENT_PLATFORMS.length + ((spec.leftovers ?? []).length > 0 ? 1 : 0)
      expect(`${spec.id}:${occurrences(link, `<span class="dot `)}`).toBe(`${spec.id}:${expected}`)
    }
  })
})

describe(`the Tier A pickers (EXP-941)`, () => {
  test(`the combobox demo shows BOTH selection languages and the none row`, () => {
    const markup = islandBody(`combobox`)
    // Single select marks the picked row with a trailing check…
    expect(occurrences(markup, `data-selected-glyph="check"`)).toBe(1)
    // …multi marks EVERY row with the leading circle pair, never a checkbox.
    expect(occurrences(markup, `data-selected-glyph="selected"`)).toBe(2)
    expect(occurrences(markup, `data-selected-glyph="unselected"`)).toBe(1)
    // …and a row that is on SOME of the edited issues wears the third glyph
    // (EXP-957), never a Minus beside a blank gutter.
    expect(occurrences(markup, `data-selected-glyph="indeterminate"`)).toBe(1)
    expect(markup).not.toContain(`data-slot="checkbox"`)
    expect(spec(`combobox`).blurb).toContain(`ComboboxMenuItems`)
    // And "nothing picked" is a ROW that reports null, not a sentinel string.
    expect(occurrences(markup, `data-combobox-none="true"`)).toBe(1)
    // A closed portal renders nothing, so the demo carries the trigger AND
    // the bare list (the icon-picker pattern).
    expect(occurrences(markup, `data-slot="combobox-list"`)).toBe(2)
    expect(markup).toContain(`data-slot="popover-trigger"`)
    expect(spec(`combobox`).blurb).toContain(`ui-selected`)
  })

  test(`the search field shows the glyph always and the clear only when filled`, () => {
    const markup = islandBody(`search-field`)
    expect(occurrences(markup, `data-slot="search-field"`)).toBe(3)
    // Two of the three carry a value, and only those two draw a clear.
    expect(occurrences(markup, `data-slot="search-field-clear"`)).toBe(2)
    expect(markup).toContain(`lucide-search`)
    // The dense rung is the Reviews file filter, named by the contract.
    expect(markup).toContain(contract.diffUi.filterPlaceholder)
  })

  test(`the segmented control is the component now, in both its forms`, () => {
    const markup = islandBody(`segmented`)
    expect(markup).toContain(`data-slot="segmented-control"`)
    // The embedded arm is a glass group's first row, not a second component.
    expect(markup).toContain(`data-slot="glass-tabs-row"`)
    expect(occurrences(markup, `data-slot="tabs-trigger"`)).toBe(5)
    const web = COMPONENTS.find((entry) => entry.id === `segmented`)?.status.web
    expect(web?.symbol).toBe(`SegmentedControl`)
    expect(web?.file).toBe(`packages/ui/src/segmented-control.tsx`)
  })

  test(`the date picker shows both trigger states and the grid they open`, () => {
    const markup = islandBody(`date-picker`)
    // `Mar 8` is formatDateLabel's own output — a literal here would be the
    // one place the page could disagree with the component.
    expect(markup).toContain(formatDateLabel(new Date(`2026-03-08T00:00:00`)))
    expect(markup).toContain(`Due date`)
    expect(occurrences(markup, `data-slot="calendar"`)).toBe(1)
  })

  test(`the typeahead demo is a menu under a field, with one active row`, () => {
    const markup = islandBody(`typeahead`)
    expect(occurrences(markup, `data-slot="typeahead-menu"`)).toBe(1)
    expect(markup).toContain(`data-placement="below"`)
    expect(occurrences(markup, `data-slot="typeahead-row"`)).toBe(3)
    expect(occurrences(markup, `aria-selected="true"`)).toBe(1)
  })

  test(`the alert demo shows both variants, and the glyph earns its column`, () => {
    const markup = islandBody(`alert`)
    expect(occurrences(markup, `data-slot="alert"`)).toBe(2)
    expect(occurrences(markup, `data-slot="alert-title"`)).toBe(1)
    expect(occurrences(markup, `data-slot="alert-description"`)).toBe(2)
    expect(markup).toContain(`text-destructive`)
  })
})
