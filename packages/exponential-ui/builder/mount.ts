// VAPP-92: the theme builder page — pick a base, tweak tokens and recipes
// with the whole catalog previewed live, import a shadcn `globals.css` or a
// tweakcn export as a start, export the JSON. Plain DOM for the editor; the
// engine is src/builder.ts, the preview is the REAL React renderer
// (builder/preview.tsx on `@exponential-at/ui-react`, VAPP-87).
// ui.exponential.at (VAPP-93) mounts it: `mountBuilder(root, modeTarget)`
// wires the page's markup (index.html's body: #base, #panel, #preview, …)
// found under `root`; main.ts runs it on the standalone page, which
// `bun run --filter @exponential-at/ui build:builder` bundles.

import { TOKEN_GROUPS } from "../src/catalog"
import { diffTheme, exportThemeJson, importShadcnCss, parseThemeJson, themeFromImport } from "../src/builder"
import { recipeParts } from "../src/recipes"
import { THEME_SCHEMA_ID, tryLoadTheme } from "../src/theme"
import { BUILTIN_THEMES, builtinTheme } from "../src/themes"
import type { ModeName, ResolvedTheme, ThemeIssue, ThemeSource } from "../src/theme-types"
import { renderPreview as mountPreview } from "./preview"

/** Start the builder on the markup under `root` (the ids of index.html's
 *  body); `modeTarget` gets `data-mode` (the `.xb` root the CSS keys on). */
export function mountBuilder(root: ParentNode = document, modeTarget: HTMLElement = document.body): void {
  const STORAGE = `exponential-ui.theme-builder`
  const $ = <T extends HTMLElement>(sel: string) => root.querySelector(sel) as T

  interface State {
    base: string
    mode: ModeName
    width: number
    draft: ThemeSource
    theme: ResolvedTheme
    issues: ThemeIssue[]
    recipeComponent: string
  }

  function freshDraft(base: string): ThemeSource {
    return { $schema: THEME_SCHEMA_ID, id: `my-theme`, name: `My theme`, extends: base }
  }

  function load(): State {
    let base = `exponential`
    let draft = freshDraft(base)
    try {
      const saved = JSON.parse(localStorage.getItem(STORAGE) ?? `null`) as { base: string; draft: ThemeSource; mode?: ModeName } | null
      if (saved?.draft && BUILTIN_THEMES.some((t) => t.id === saved.base)) {
        base = saved.base
        draft = saved.draft
      }
      const { theme, issues } = tryLoadTheme(draft, { themes: BUILTIN_THEMES })
      return { base, mode: saved?.mode ?? `dark`, width: 900, draft, theme: theme ?? builtinTheme(base), issues, recipeComponent: `Button` }
    } catch {
      return { base, mode: `dark`, width: 900, draft, theme: builtinTheme(base), issues: [], recipeComponent: `Button` }
    }
  }

  const state = load()

  function save(): void {
    try {
      localStorage.setItem(STORAGE, JSON.stringify({ base: state.base, draft: state.draft, mode: state.mode }))
    } catch {
      /* private mode */
    }
  }

  /** Re-resolve the draft; keep the last good theme when it fails. */
  function reload(): void {
    const { theme, issues } = tryLoadTheme(state.draft, { themes: BUILTIN_THEMES })
    state.issues = issues
    if (theme) state.theme = theme
    save()
    renderIssues()
    renderPreview()
  }

  // ---------------------------------------------------------------------------
  // Draft edits
  // ---------------------------------------------------------------------------

  function setColor(mode: ModeName, name: string, hex: string): void {
    const modes = (state.draft.modes ??= {})
    const m = (modes[mode] ??= {})
    const color = (m.color ??= {})
    const baseValue = builtinTheme(state.base).modes[mode].color[name]
    if (hex === baseValue) delete color[name]
    else color[name] = hex as `#${string}`
    if (Object.keys(color).length === 0) delete m.color
    reload()
  }

  function setToken(group: string, name: string, value: number | string): void {
    const tokens = (state.draft.tokens ??= {}) as Record<string, Record<string, unknown>>
    const [top, sub] = group.split(`.`)
    const baseTable = sub
      ? (builtinTheme(state.base).tokens.type as unknown as Record<string, Record<string, unknown>>)[sub]
      : (builtinTheme(state.base).tokens as unknown as Record<string, Record<string, unknown>>)[top]
    const table = sub ? (((tokens.type ??= {}) as Record<string, Record<string, unknown>>)[sub] ??= {}) : (tokens[top] ??= {})
    if (baseTable[name] === value) delete table[name]
    else table[name] = value
    reload()
  }

  function setFont(family: string, fallback: string): void {
    const fonts = (state.draft.fonts ??= {})
    fonts[family] = { ...(fonts[family] ?? {}), fallback, source: `host` }
    reload()
  }

  function setRecipes(component: string, json: string): string | null {
    let parsed: unknown
    try {
      parsed = json.trim() === `` ? undefined : JSON.parse(json)
    } catch (error) {
      return (error as Error).message
    }
    const recipes = (state.draft.recipes ??= {})
    if (parsed === undefined) delete recipes[component]
    else recipes[component] = parsed as Record<string, never>
    if (Object.keys(recipes).length === 0) delete state.draft.recipes
    reload()
    return null
  }

  // ---------------------------------------------------------------------------
  // Panel
  // ---------------------------------------------------------------------------

  function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Record<string, string> = {}, ...children: (Node | string)[]): HTMLElementTagNameMap[K] {
    const e = document.createElement(tag)
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v)
    for (const c of children) e.append(c)
    return e
  }

  function section(title: string, body: HTMLElement, open = false): HTMLElement {
    const d = h(`details`, open ? { open: `` } : {}, h(`summary`, {}, title), body)
    return d
  }

  function colorRows(mode: ModeName): HTMLElement {
    const body = h(`div`, { class: `rows` })
    for (const name of TOKEN_GROUPS.color) {
      const value = state.theme.modes[mode].color[name]
      const rgb = value.slice(0, 7)
      const picker = h(`input`, { type: `color`, value: rgb }) as HTMLInputElement
      const hex = h(`input`, { type: `text`, value, class: `mono`, spellcheck: `false` }) as HTMLInputElement
      picker.addEventListener(`input`, () => {
        const alpha = value.length === 9 ? value.slice(7) : ``
        hex.value = `${picker.value}${alpha}`
        setColor(mode, name, hex.value)
      })
      hex.addEventListener(`change`, () => {
        if (/^#([0-9a-f]{6}|[0-9a-f]{8})$/.test(hex.value)) {
          picker.value = hex.value.slice(0, 7)
          setColor(mode, name, hex.value)
        } else hex.classList.add(`bad`)
      })
      body.append(h(`label`, { class: `row` }, h(`span`, { class: `name` }, name), picker, hex))
    }
    return body
  }

  function numberRows(group: string): HTMLElement {
    const body = h(`div`, { class: `rows` })
    const [top, sub] = group.split(`.`)
    const table = sub ? (state.theme.tokens.type as unknown as Record<string, Record<string, number>>)[sub] : (state.theme.tokens as unknown as Record<string, Record<string, number>>)[top]
    for (const name of TOKEN_GROUPS[group]) {
      const input = h(`input`, { type: `number`, value: String(table[name]), step: group === `opacity` || group === `density` ? `0.05` : `1` }) as HTMLInputElement
      input.addEventListener(`change`, () => setToken(group, name, Number(input.value)))
      body.append(h(`label`, { class: `row` }, h(`span`, { class: `name` }, name), input))
    }
    return body
  }

  function fontRows(): HTMLElement {
    const body = h(`div`, { class: `rows` })
    for (const slot of TOKEN_GROUPS[`type.family`]) {
      const family = state.theme.tokens.type.family[slot]
      const spec = state.theme.fonts[family] ?? {}
      const fam = h(`input`, { type: `text`, value: family }) as HTMLInputElement
      const fb = h(`input`, { type: `text`, value: spec.fallback ?? ``, placeholder: `fallback stack` }) as HTMLInputElement
      const apply = () => {
        const name = fam.value.trim()
        if (!name) return
        setFont(name, fb.value.trim())
        setToken(`type.family`, slot, name)
      }
      fam.addEventListener(`change`, apply)
      fb.addEventListener(`change`, apply)
      body.append(h(`label`, { class: `row` }, h(`span`, { class: `name` }, slot), fam), h(`label`, { class: `row` }, h(`span`, { class: `name muted` }, `fallback`), fb))
    }
    return body
  }

  function recipeEditor(): HTMLElement {
    const parts = recipeParts()
    const select = h(`select`) as HTMLSelectElement
    for (const name of Object.keys(parts)) select.append(h(`option`, { value: name, ...(name === state.recipeComponent ? { selected: `` } : {}) }, name))
    const hint = h(`div`, { class: `hint mono` })
    const area = h(`textarea`, { rows: `14`, spellcheck: `false`, class: `mono` }) as HTMLTextAreaElement
    const error = h(`div`, { class: `error` })
    const fill = () => {
      const component = select.value
      state.recipeComponent = component
      hint.textContent = `parts: ${parts[component].parts.join(`, `)}  ·  when: state, ${parts[component].props.join(`, `) || `(none)`}`
      const own = state.draft.recipes?.[component]
      area.value = own ? JSON.stringify(own, null, 2) : ``
      area.placeholder = `{\n  "${parts[component].parts[0]}": [\n    { "style": { "borderRadius": "$radius.full" } },\n    { "when": { "state": "hover" }, "style": { "opacity": 0.9 } }\n  ]\n}`
      error.textContent = ``
    }
    select.addEventListener(`change`, fill)
    area.addEventListener(`change`, () => {
      const err = setRecipes(select.value, area.value)
      error.textContent = err ?? ``
    })
    const base = h(`details`, {}, h(`summary`, {}, `What the base theme says`), h(`pre`, { class: `mono base-rules` }))
    const showBase = () => {
      ;(base.querySelector(`pre`) as HTMLElement).textContent = JSON.stringify(builtinTheme(state.base).recipes[select.value] ?? {}, null, 1)
    }
    select.addEventListener(`change`, showBase)
    base.addEventListener(`toggle`, showBase)
    fill()
    return h(`div`, { class: `rows` }, select, hint, area, error, base)
  }

  function renderPanel(): void {
    const panel = $(`#panel`)
    panel.replaceChildren()
    const id = h(`input`, { type: `text`, value: state.draft.id }) as HTMLInputElement
    const name = h(`input`, { type: `text`, value: state.draft.name }) as HTMLInputElement
    id.addEventListener(`change`, () => { state.draft.id = id.value.trim(); reload() })
    name.addEventListener(`change`, () => { state.draft.name = name.value.trim(); reload() })
    panel.append(section(`Theme`, h(`div`, { class: `rows` }, h(`label`, { class: `row` }, h(`span`, { class: `name` }, `id`), id), h(`label`, { class: `row` }, h(`span`, { class: `name` }, `name`), name), h(`div`, { class: `hint` }, `extends ${state.base}`)), true))
    panel.append(section(`Colours · ${state.mode}`, colorRows(state.mode), true))
    panel.append(section(`Radius`, numberRows(`radius`)))
    panel.append(section(`Spacing`, numberRows(`spacing`)))
    panel.append(section(`Control heights`, numberRows(`control`)))
    panel.append(section(`Type · size`, numberRows(`type.size`)))
    panel.append(section(`Type · line height`, numberRows(`type.lineHeight`)))
    panel.append(section(`Type · weight`, numberRows(`type.weight`)))
    panel.append(section(`Fonts`, fontRows()))
    panel.append(section(`Border widths`, numberRows(`border`)))
    panel.append(section(`Opacity`, numberRows(`opacity`)))
    panel.append(section(`Motion (ms)`, numberRows(`motion`)))
    panel.append(section(`Breakpoints (px)`, numberRows(`breakpoint`)))
    panel.append(section(`Density (× control + spacing)`, numberRows(`density`)))
    panel.append(section(`Recipes`, recipeEditor(), true))
  }

  function renderIssues(): void {
    const box = $(`#issues`)
    box.replaceChildren()
    box.hidden = state.issues.length === 0
    for (const issue of state.issues) box.append(h(`div`, { class: `issue` }, h(`code`, {}, issue.path), ` ${issue.message}`))
  }

  // ---------------------------------------------------------------------------
  // Preview: the recipe sheet + the kitchen sink (builder/preview.tsx)
  // ---------------------------------------------------------------------------

  function renderPreview(): void {
    const host = $(`#preview`)
    const bg = state.theme.modes[state.mode].color.background
    const fg = state.theme.modes[state.mode].color.foreground
    host.style.cssText = `background:${bg};color:${fg};width:${state.width}px`
    modeTarget.dataset.mode = state.mode
    mountPreview(host, { theme: state.theme, mode: state.mode, width: state.width })
  }

  // ---------------------------------------------------------------------------
  // Toolbar
  // ---------------------------------------------------------------------------

  function renderToolbar(): void {
    const base = $(`#base`) as HTMLSelectElement
    base.replaceChildren(...BUILTIN_THEMES.map((t) => h(`option`, { value: t.id, ...(t.id === state.base ? { selected: `` } : {}) }, `${t.name} (${t.id})`)))
    base.addEventListener(`change`, () => {
      state.base = base.value
      state.draft.extends = state.base
      reload()
      renderPanel()
    })
    for (const b of root.querySelectorAll<HTMLButtonElement>(`[data-set-mode]`)) {
      b.setAttribute(`aria-pressed`, String(b.dataset.setMode === state.mode))
      b.addEventListener(`click`, () => {
        state.mode = b.dataset.setMode as ModeName
        for (const o of root.querySelectorAll<HTMLButtonElement>(`[data-set-mode]`)) o.setAttribute(`aria-pressed`, String(o === b))
        save()
        renderPanel()
        renderPreview()
      })
    }
    const width = $(`#width`) as HTMLSelectElement
    width.value = String(state.width)
    width.addEventListener(`change`, () => {
      state.width = Number(width.value)
      renderPreview()
    })
    $(`#reset`).addEventListener(`click`, () => {
      state.draft = freshDraft(state.base)
      reload()
      renderPanel()
    })
    const exportBox = $(`#export`) as HTMLTextAreaElement
    $(`#do-export`).addEventListener(`click`, () => {
      const minimal = diffTheme(state.theme, builtinTheme(state.base), { id: state.draft.id, name: state.draft.name })
      const text = exportThemeJson(minimal)
      exportBox.value = text
      exportBox.hidden = false
      const a = document.createElement(`a`)
      a.href = URL.createObjectURL(new Blob([text], { type: `application/json` }))
      a.download = `${state.draft.id || `theme`}.theme.json`
      a.click()
    })
    const importDialog = $(`#import`) as HTMLDialogElement
    $(`#do-import`).addEventListener(`click`, () => importDialog.showModal())
    $(`#import-apply`).addEventListener(`click`, () => {
      const text = ($(`#import-text`) as HTMLTextAreaElement).value
      const report = $(`#import-report`)
      if (text.trim().startsWith(`{`)) {
        const parsed = parseThemeJson(text, BUILTIN_THEMES)
        if (!parsed.source) {
          report.textContent = parsed.issues.map((i) => `${i.path}: ${i.message}`).join(`\n`)
          return
        }
        if (parsed.source.extends && BUILTIN_THEMES.some((t) => t.id === parsed.source!.extends)) state.base = parsed.source.extends
        state.draft = { ...parsed.source, extends: state.base }
        report.textContent = parsed.issues.length ? parsed.issues.map((i) => `${i.path}: ${i.message}`).join(`\n`) : `Theme loaded.`
      } else {
        const imp = importShadcnCss(text)
        const found = Object.values(imp.modes).reduce((n, m) => n + Object.keys(m?.color ?? {}).length, 0)
        if (found === 0) {
          report.textContent = `No shadcn variables found. Paste the :root / .dark block of globals.css or a tweakcn export.`
          return
        }
        const source = themeFromImport(imp, { id: state.draft.id, name: state.draft.name, extends: state.base })
        state.draft = { ...source, recipes: state.draft.recipes }
        report.textContent = `Imported ${found} colours${imp.tokens.radius ? `, the radius ladder` : ``}${imp.tokens.type?.family ? `, fonts` : ``}.${imp.unmapped.length ? ` Not mapped: ${imp.unmapped.join(`, `)}` : ``}`
      }
      reload()
      renderPanel()
    })
    $(`#import-close`).addEventListener(`click`, () => importDialog.close())
  }

  renderToolbar()
  renderPanel()
  renderIssues()
  renderPreview()
}
