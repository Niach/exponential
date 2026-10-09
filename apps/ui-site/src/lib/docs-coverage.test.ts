/* Every catalog component is documented: a page, a specimen, a summary
   that fits one line of a search snippet (<= 160 chars), no lorem ipsum
   anywhere it renders, and only real components in its links. */
import { expect, test } from "bun:test"
import coreCatalog from "@exponential-at/ui/catalog/core.catalog.json"
import { embedCode, EMBED_TARGETS, jsonl, type EmbedTarget } from "../sdk/embed"
import { surfaceMessages, flatten } from "../sdk/a2ui"
import { studioTree, subjectOf, thumbProps } from "../sdk/ComponentStudio"
import { galleryVariants, hasVariants } from "../sdk/VariantGallery"
import { COMPONENT_DOCS, DEPRECATED_ALIASES, ORDERED_DOCS, aliasesOf, SUMMARY_MAX, componentPath, componentSummary, keyboardRows, macroParts, relatedComponents, specimenById } from "./catalog"
import { ROUTES } from "./routes"

/* Every authorable component (`hidden` ones, the renderer's Unknown
   placeholder and a Row's TreeGuides part, are never authored and get no
   page; nor do the one-release `deprecated` aliases). */
const catalogNames = Object.entries((coreCatalog as { components: Record<string, { hidden?: boolean; deprecated?: string }> }).components)
  .filter(([, c]) => !c.hidden && !c.deprecated)
  .map(([name]) => name)
const LOREM = /lorem|ipsum|dolor sit|consectetur/i

test(`every catalog component has a doc, a page and a specimen`, () => {
  const paths = new Set(ROUTES.map((r) => r.path))
  expect(catalogNames.length).toBeGreaterThan(0)
  for (const name of catalogNames) {
    const doc = COMPONENT_DOCS.find((d) => d.name === name)
    expect(doc, name).toBeDefined()
    expect(paths.has(componentPath(doc!)), name).toBe(true)
    const specimen = specimenById(doc!.specimenId)
    expect(specimen, name).toBeDefined()
    expect(subjectOf(doc!, specimen)?.component, name).toBe(name)
  }
  expect(ORDERED_DOCS.length).toBe(COMPONENT_DOCS.length)
  // …and nothing else: no `hidden` component reaches the index, the sidebar or a route.
  for (const doc of ORDERED_DOCS) expect(catalogNames, doc.name).toContain(doc.name)
})

test(`every summary is one line of <= 160 chars`, () => {
  for (const doc of COMPONENT_DOCS) {
    const s = componentSummary(doc)
    expect(s.length, doc.name).toBeLessThanOrEqual(SUMMARY_MAX)
    expect(s.length, doc.name).toBeGreaterThan(15)
    expect(s.includes(`\n`), doc.name).toBe(false)
  }
})

test(`no lorem ipsum in descriptions, examples or specimens`, () => {
  for (const doc of COMPONENT_DOCS) {
    expect(LOREM.test(JSON.stringify(doc)), doc.name).toBe(false)
    expect(LOREM.test(JSON.stringify(specimenById(doc.specimenId))), doc.name).toBe(false)
  }
})

test(`links name real components; the keyboard rows parse`, () => {
  for (const doc of COMPONENT_DOCS) {
    for (const r of relatedComponents(doc)) expect(r, doc.name).toBeDefined()
    for (const p of macroParts(doc.name)) expect(catalogNames.includes(p), `${doc.name} → ${p}`).toBe(true)
    if (doc.kind === `macro`) expect(macroParts(doc.name).length, doc.name).toBeGreaterThan(0)
    for (const k of keyboardRows(doc)) expect(k.action.length, doc.name).toBeGreaterThan(0)
  }
})

test(`variants change exactly one prop and stay valid trees`, () => {
  for (const doc of COMPONENT_DOCS) {
    const specimen = specimenById(doc.specimenId)
    const subject = subjectOf(doc, specimen)
    const base = thumbProps(doc, subject)
    const variants = galleryVariants(doc, specimen)
    // The page's Variants section shows exactly when the gallery has entries.
    expect(hasVariants(doc, specimen), doc.name).toBe(variants.length > 0)
    expect(new Set(variants.map((v) => v.label)).size, doc.name).toBe(variants.length)
    const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)
    for (const v of variants) {
      expect(v.base, doc.name).toEqual(base)
      const changed = Object.keys({ ...base, ...v.props }).filter((k) => !same(v.props[k], base[k]))
      // The current value stays on purpose (it labels the base render); every other option changes its prop, and only it.
      expect(changed, `${doc.name} ${v.label}`).toEqual(same(v.props[v.prop], base[v.prop]) ? [] : [v.prop])
      expect(flatten(v.tree)[0]!.id).toBe(`root`)
    }
    // Never a gallery of base copies.
    if (variants.length) expect(variants.some((v) => !same(v.props[v.prop], base[v.prop])), doc.name).toBe(true)
  }
})

/* Each snippet's string literal, read back the way its compiler would. */
const embedded: Record<EmbedTarget, (code: string) => string> = {
  // A JS template literal: evaluate it.
  react: (code) => new Function(`return \`${code.match(/const SURFACE = `([\s\S]*?)`\n\nconst transport/)![1]}\``)() as string,
  // A Swift raw multi-line string: no `\#…` escape and no closing delimiter inside.
  swiftui: (code) => {
    const [, h, body] = code.match(/let surface = (#+)"""\n([\s\S]*?)\n"""\1\n/)!
    expect(body!.includes(`\\${h}`)).toBe(false)
    return body!
  },
  // A Kotlin raw string: `${'$'}` is the only `$`.
  compose: (code) => {
    const body = code.match(/private val SURFACE = """\n([\s\S]*?)\n"""\n/)![1]!
    expect(body.split(`\${'$'}`).join(``).includes(`$`)).toBe(false)
    expect(body.includes(`"""`)).toBe(false)
    return body.split(`\${'$'}`).join(`$`)
  },
  // A Rust raw string: no escapes at all.
  gpui: (code) => {
    const [, h, body] = code.match(/r(#+)"\n([\s\S]*?)\n"\1;/)!
    expect(body!.includes(`"${h}`)).toBe(false)
    return body!
  },
}

test(`embed code round-trips the surface on every platform`, () => {
  const doc = COMPONENT_DOCS.find((d) => d.name === `Button`)!
  const specimen = specimenById(doc.specimenId)
  const nasty = [`Say "hi" \`now\` \${x} $y`, `a"#b "##c`, `"""# \\#(x) \\(y)`, `back\\slash \\n`, `ünï ✓ 🙂`]
  for (const label of nasty) {
    const messages = surfaceMessages(`preview`, flatten(studioTree(doc, subjectOf(doc, specimen), { label })))
    for (const t of EMBED_TARGETS) {
      const code = embedCode(t.id, { messages, surfaceId: `preview`, theme: `playful`, mode: `light` })
      expect(embedded[t.id](code), `${t.id} ${label}`).toBe(jsonl(messages))
      expect(code, t.id).toContain(`playful`)
      expect(code, t.id).toContain(`"preview"`)
    }
  }
})

test(`embed code follows the studio's direction and width`, () => {
  const messages = surfaceMessages(`preview`, [{ id: `root`, component: `Text`, text: `x` }])
  const opts = { messages, surfaceId: `preview`, theme: `neutral`, mode: `dark` as const }
  for (const t of EMBED_TARGETS) {
    const plain = embedCode(t.id, opts)
    const rtl = embedCode(t.id, { ...opts, direction: `rtl` })
    const sized = embedCode(t.id, { ...opts, width: 390 })
    expect(plain.includes(`rtl`) || plain.includes(`RTL`), t.id).toBe(false)
    expect(rtl.includes(`rtl`) || rtl.includes(`RTL`), t.id).toBe(true)
    expect(plain.includes(`390`), t.id).toBe(false)
    expect(sized.includes(`390`), t.id).toBe(true)
  }
  expect(embedCode(`react`, { ...opts, direction: `rtl`, width: 390 })).toContain(`direction="rtl" width={390}`)
})

test(`index search matches word starts`, async () => {
  const { matches } = await import(`../pages/ComponentsIndex`)
  const by = (q: string, group = `all`) => COMPONENT_DOCS.filter((d) => matches(d, q, group)).map((d) => d.name)
  expect(by(`date`)).toEqual(expect.arrayContaining([`DatePicker`, `DateRangePicker`]))
  expect(by(`date`)).not.toContain(`Input`)
  expect(by(`picker`)).toContain(`TimePicker`)
  expect(by(``, `overlay`).length).toBe(COMPONENT_DOCS.filter((d) => d.group === `overlay`).length)
  expect(by(`zzzz`)).toEqual([])
})

test(`a deprecated alias gets no page; its replacement's page lists it`, () => {
  const paths = new Set(ROUTES.map((r) => r.path))
  expect(Object.keys(DEPRECATED_ALIASES).length).toBeGreaterThan(0)
  for (const [alias, to] of Object.entries(DEPRECATED_ALIASES)) {
    expect(COMPONENT_DOCS.some((d) => d.name === alias), alias).toBe(false)
    expect(paths.has(`/components/${alias.replace(/([a-z0-9])([A-Z])/g, `$1-$2`).toLowerCase()}/`), alias).toBe(false)
    const doc = COMPONENT_DOCS.find((d) => d.name === to)
    expect(doc, `${alias} → ${to}`).toBeDefined()
    expect(aliasesOf(to)).toContain(alias)
  }
})
