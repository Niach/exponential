// VAPP-92: the theme builder's PREVIEW painter — a normalized tree + a
// resolved theme → DOM, every visual read off the recipes (src/theme.ts
// resolveNodeStyle) so a theme edit shows up at once. It exists for the
// builder page only: the React renderer (VAPP-87) is the web painter; this
// one paints icons as a placeholder glyph, shows overlays inline and skips
// media, data bindings and events.

import { nativeRecipeProps } from "../src/recipes"
import { resolveNodeStyle, resolveRecipe } from "../src/theme"
import { styleToCss } from "../src/css"
import type { ModeName, ResolvedStyle, ResolvedTheme } from "../src/theme-types"
import type { UiNode } from "../src/types"

export interface PaintContext {
  theme: ResolvedTheme
  mode: ModeName
  /** The surface width the `@media (min-width)` blocks resolve against. */
  width: number
  /** Interaction states forced on every node (the recipe sheet's columns). */
  states?: readonly string[]
}

const MEDIA = /^@media \(min-width: (\d+(?:\.\d+)?)px\)$/

/** Nested condition blocks flattened for the preview width; `:pressed` dropped. */
function flatten(style: ResolvedStyle, width: number): ResolvedStyle {
  const out: ResolvedStyle = {}
  const later: ResolvedStyle[] = []
  for (const [key, value] of Object.entries(style)) {
    const m = MEDIA.exec(key)
    if (m) {
      if (width >= parseFloat(m[1])) later.push(value as unknown as ResolvedStyle)
      continue
    }
    if (key.startsWith(`:`)) continue
    out[key] = value
  }
  for (const block of later) Object.assign(out, block)
  return out
}

function applyCss(el: HTMLElement, style: ResolvedStyle, ctx: PaintContext): void {
  for (const [k, v] of Object.entries(styleToCss(flatten(style, ctx.width), ctx.theme.fonts))) el.style.setProperty(k, v)
}

function text(value: unknown): string {
  if (value === undefined || value === null) return ``
  if (typeof value === `object`) return `path` in (value as object) ? `{${(value as { path: string }).path}}` : JSON.stringify(value)
  return String(value)
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag)
  if (cls) e.className = cls
  return e
}

/** A part of a native, painted from its recipe alone. */
function part(ctx: PaintContext, node: UiNode, name: string, tag: keyof HTMLElementTagNameMap = `div`, states: readonly string[] = []): HTMLElement {
  const e = el(tag)
  const props = nativeRecipeProps(node.component, node.props)
  applyCss(e, resolveRecipe(ctx.theme, { component: node.component, part: name, props, states: [...(ctx.states ?? []), ...states] }, ctx.mode), ctx)
  e.dataset.part = `${node.component}/${name}`
  return e
}

function icon(_ctx: PaintContext, size: number, color?: string): HTMLElement {
  const wrap = el(`span`)
  wrap.style.cssText = `display:inline-flex;flex-shrink:0;width:${size}px;height:${size}px;${color ? `color:${color}` : ``}`
  wrap.innerHTML = `<svg viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M9 12l2 2 4-4"/></svg>`
  return wrap
}

function hover(e: HTMLElement, paintStates: (states: string[]) => void, base: string[] = []): void {
  e.addEventListener(`mouseenter`, () => paintStates([...base, `hover`]))
  e.addEventListener(`mouseleave`, () => paintStates(base))
  e.addEventListener(`mousedown`, () => paintStates([...base, `pressed`]))
  e.addEventListener(`mouseup`, () => paintStates([...base, `hover`]))
}

function label(ctx: PaintContext, node: UiNode, partName: string, value: unknown, tag: keyof HTMLElementTagNameMap = `span`): HTMLElement | null {
  const s = text(value)
  if (!s) return null
  const e = part(ctx, node, partName, tag)
  e.textContent = s
  return e
}

function paintChildren(ctx: PaintContext, node: UiNode, into: HTMLElement): void {
  for (const child of node.children) into.appendChild(paint(child, ctx))
  if (node.template) {
    const hint = el(`div`)
    hint.textContent = `× ${node.template.component} per item at ${node.template.path}`
    hint.style.cssText = `font:12px ui-monospace,monospace;opacity:.6;padding:4px`
    into.appendChild(hint)
  }
}

const paintNative: Record<string, (node: UiNode, ctx: PaintContext, root: HTMLElement) => void> = {
  Box(node, ctx, root) {
    if (!node.style?.display) root.style.display = `flex`, (root.style.flexDirection ||= `column`)
    paintChildren(ctx, node, root)
    if (node.props.pressable) hover(root, (states) => applyCss(root, resolveNodeStyle(ctx.theme, node, ctx.mode, states), ctx))
  },
  List(node, ctx, root) {
    root.style.display = `flex`
    root.style.flexDirection = node.props.direction === `horizontal` ? `row` : `column`
    root.style.overflow = `auto`
    node.children.forEach((child, i) => {
      if (i > 0 && node.props.divided) root.appendChild(part(ctx, node, `divider`))
      root.appendChild(paint(child, ctx))
    })
  },
  Text(node, _ctx, root) {
    root.textContent = text(node.props.text)
    if (node.props.align) root.style.textAlign = node.props.align === `end` ? `right` : node.props.align === `center` ? `center` : `left`
    if (node.props.lines === 1) root.style.cssText += `;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0`
  },
  Markdown(node, ctx, root) {
    const pre = part(ctx, node, `paragraph`, `div`)
    pre.style.whiteSpace = `pre-wrap`
    pre.textContent = text(node.props.text)
    root.appendChild(pre)
  },
  Image(node, _ctx, root) {
    root.style.cssText += `;background:repeating-linear-gradient(45deg,#8883 0 6px,#8881 6px 12px);display:flex;align-items:center;justify-content:center;font:11px ui-monospace,monospace;opacity:.9;min-height:24px`
    if (node.props.width) root.style.width = `${node.props.width}px`
    if (node.props.height) root.style.height = `${node.props.height}px`
    root.textContent = text(node.props.alt) || `image`
  },
  Icon(node, ctx, root) {
    const s = resolveRecipe(ctx.theme, { component: `Icon`, part: `root`, props: nativeRecipeProps(`Icon`, node.props), states: ctx.states }, ctx.mode)
    root.style.display = `inline-flex`
    root.appendChild(icon(ctx, typeof s.width === `number` ? s.width : 20))
    root.title = text(node.props.name)
  },
  Video(node, ctx, root) {
    root.style.cssText += `;aspect-ratio:16/9;background:#0008;display:flex;align-items:center;justify-content:center;color:#fff;font:12px system-ui`
    root.textContent = `▶ video`
    root.appendChild(part(ctx, node, `controls`))
  },
  AudioPlayer(node, ctx, root) {
    root.style.cssText += `;display:flex;align-items:center;gap:8px`
    root.appendChild(icon(ctx, 16))
    const track = part(ctx, node, `track`)
    track.style.cssText += `;flex:1;height:4px;background:currentColor;opacity:.3;border-radius:2px`
    root.appendChild(track)
  },
  Avatar(node, ctx, root) {
    const fb = part(ctx, node, `fallback`)
    fb.style.cssText += `;width:100%;height:100%;border-radius:inherit;display:flex;align-items:center;justify-content:center`
    fb.textContent = text(node.props.name).split(/\s+/).map((w) => w[0]).join(``).slice(0, 2).toUpperCase() || `?`
    root.style.cssText += `;overflow:hidden;flex-shrink:0;display:flex`
    root.appendChild(fb)
  },
  Carousel(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column;gap:8px`
    const row = el(`div`)
    row.style.cssText = `display:flex;gap:8px;overflow:auto`
    for (const child of node.children) {
      const page = part(ctx, node, `page`)
      page.style.cssText += `;flex:0 0 80%`
      page.appendChild(paint(child, ctx))
      row.appendChild(page)
    }
    root.appendChild(row)
    if (node.props.indicators !== false) {
      const dots = el(`div`)
      dots.style.cssText = `display:flex;gap:6px;justify-content:center`
      node.children.forEach((_, i) => dots.appendChild(part(ctx, node, `indicator`, `span`, i === 0 ? [`selected`] : [])))
      root.appendChild(dots)
    }
  },
  Tabs(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column`
    const list = part(ctx, node, `list`)
    list.style.cssText += `;display:flex;align-self:${node.props.fill ? `stretch` : `flex-start`}`
    const tabs = (node.props.tabs as { label: string; value: string }[] | undefined) ?? []
    for (const tab of tabs) {
      const selected = tab.value === node.props.value
      const t = part(ctx, node, `tab`, `button`, selected ? [`selected`] : [])
      t.style.cssText += `;display:inline-flex;align-items:center;justify-content:center;flex:${node.props.fill ? 1 : `none`};cursor:pointer;flex-direction:column`
      t.textContent = text(tab.label)
      if (selected) {
        const ind = part(ctx, node, `indicator`)
        ind.style.cssText += `;align-self:stretch`
        t.appendChild(ind)
      }
      hover(t, (states) => applyCss(t, resolveRecipe(ctx.theme, { component: `Tabs`, part: `tab`, props: nativeRecipeProps(`Tabs`, node.props), states: selected ? [...states, `selected`] : states }, ctx.mode), ctx))
      list.appendChild(t)
    }
    root.appendChild(list)
    const content = part(ctx, node, `content`)
    paintChildren(ctx, node, content)
    root.appendChild(content)
  },
  ToggleGroup(node, ctx, root) {
    root.style.cssText += `;display:inline-flex;align-items:center;align-self:${node.props.fill ? `stretch` : `flex-start`}`
    const items = (node.props.items as { label: string; value: string; icon?: string }[] | undefined) ?? []
    const value = node.props.value
    for (const item of items) {
      const selected = Array.isArray(value) ? value.includes(item.value) : item.value === value
      const b = part(ctx, node, `item`, `button`, selected ? [`selected`] : [])
      b.style.cssText += `;display:inline-flex;align-items:center;justify-content:center;gap:6px;cursor:pointer;flex:${node.props.fill ? 1 : `none`}`
      if (item.icon) b.appendChild(icon(ctx, 16))
      if (item.label) b.appendChild(document.createTextNode(item.label))
      root.appendChild(b)
    }
  },
  Accordion(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column`
    const items = (node.props.items as { title: string; value: string; count?: number }[] | undefined) ?? []
    items.forEach((item, i) => {
      const open = node.props.value === item.value || (Array.isArray(node.props.value) && node.props.value.includes(item.value))
      const it = part(ctx, node, `item`, `div`, open ? [`open`] : [])
      it.style.cssText += `;border-left:0;border-right:0;border-top:0;display:flex;flex-direction:column`
      const trig = part(ctx, node, `trigger`, `button`, open ? [`open`] : [])
      trig.style.cssText += `;display:flex;align-items:center;justify-content:space-between;background:none;border:0;cursor:pointer;text-align:left;width:100%`
      trig.textContent = `${text(item.title)}${item.count !== undefined ? ` · ${item.count}` : ``}`
      trig.appendChild(icon(ctx, 16))
      it.appendChild(trig)
      if (open) {
        const content = part(ctx, node, `content`, `div`, [`open`])
        if (i === 0) paintChildren(ctx, node, content)
        else content.textContent = `…`
        it.appendChild(content)
      }
      root.appendChild(it)
    })
  },
  Dialog(node, ctx, root) {
    overlayPreview(node, ctx, root, `Dialog`)
  },
  Drawer(node, ctx, root) {
    overlayPreview(node, ctx, root, `Drawer`)
  },
  Popover(node, ctx, root) {
    overlayPreview(node, ctx, root, `Popover`)
  },
  Tooltip(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column;gap:6px;align-items:flex-start`
    paintChildren(ctx, node, root)
    const c = part(ctx, node, `content`)
    c.textContent = text(node.props.text)
    root.appendChild(c)
  },
  DropdownMenu(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column;gap:6px;align-items:flex-start`
    if (node.slots?.trigger) root.appendChild(paint(node.slots.trigger, ctx))
    const c = part(ctx, node, `content`)
    c.style.cssText += `;display:flex;flex-direction:column;min-width:180px`
    const items = (node.props.items as { label?: string; icon?: string; separator?: boolean; destructive?: boolean; disabled?: boolean }[] | undefined) ?? []
    for (const item of items) {
      if (item.separator) {
        c.appendChild(part(ctx, node, `separator`))
        continue
      }
      const it = part(ctx, node, `item`, `div`, item.disabled ? [`disabled`] : [])
      it.style.cssText += `;display:flex;align-items:center;cursor:pointer`
      if (item.icon) it.appendChild(icon(ctx, 16))
      it.appendChild(document.createTextNode(text(item.label)))
      hover(it, (states) => applyCss(it, resolveRecipe(ctx.theme, { component: `DropdownMenu`, part: `item`, props: {}, states }, ctx.mode), ctx))
      c.appendChild(it)
    }
    root.appendChild(c)
  },
  Ring(node, ctx, root) {
    const s = resolveRecipe(ctx.theme, { component: `Ring`, part: `root`, props: nativeRecipeProps(`Ring`, node.props) }, ctx.mode)
    const size = typeof s.width === `number` ? s.width : 32
    const track = resolveRecipe(ctx.theme, { component: `Ring`, part: `track`, props: nativeRecipeProps(`Ring`, node.props) }, ctx.mode)
    const fill = resolveRecipe(ctx.theme, { component: `Ring`, part: `fill`, props: nativeRecipeProps(`Ring`, node.props) }, ctx.mode)
    const value = typeof node.props.value === `number` ? node.props.value : 0.5
    const r = size / 2 - 2
    const c = 2 * Math.PI * r
    root.innerHTML = `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}"><circle cx="${size / 2}" cy="${size / 2}" r="${r}" fill="none" stroke="${track.color ?? `#8883`}" stroke-width="${track.borderWidth ?? 2}"/><circle cx="${size / 2}" cy="${size / 2}" r="${r}" fill="none" stroke="${fill.color ?? `currentColor`}" stroke-width="${fill.borderWidth ?? 2}" stroke-dasharray="${c * value} ${c}" transform="rotate(-90 ${size / 2} ${size / 2})" stroke-linecap="round"/></svg>`
    root.style.display = `inline-flex`
  },
  Spinner(node, ctx, root) {
    const s = resolveRecipe(ctx.theme, { component: `Spinner`, part: `root`, props: nativeRecipeProps(`Spinner`, node.props) }, ctx.mode)
    const size = typeof s.width === `number` ? s.width : 20
    root.innerHTML = `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M21 12a9 9 0 1 1-6.2-8.6"/></svg>`
    root.style.display = `inline-flex`
  },
  Skeleton(node, _ctx, root) {
    root.style.width = text(node.props.width) || `100%`
    root.style.height = text(node.props.height) || `16px`
  },
  Chart(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column;gap:6px`
    const grid = part(ctx, node, `grid`)
    grid.style.cssText += `;height:120px;background:repeating-linear-gradient(0deg,currentColor 0 1px,transparent 1px 30px);opacity:.35`
    root.appendChild(grid)
    const axis = part(ctx, node, `axis`)
    axis.textContent = `${text(node.props.kind) || `bar`} chart · ${((node.props.series as unknown[]) ?? []).length} series`
    root.appendChild(axis)
  },
  Button(node, ctx, root) {
    root.style.cssText += `;display:inline-flex;align-items:center;justify-content:center;cursor:pointer;white-space:nowrap`
    if (node.props.loading) root.appendChild(part(ctx, node, `spinner`, `span`))
    else if (node.props.icon) root.appendChild(part(ctx, node, `icon`, `span`)).appendChild(icon(ctx, 16))
    if (node.props.label && node.props.size !== `icon`) root.appendChild(label(ctx, node, `label`, node.props.label)!)
    const states = node.props.disabled ? [`disabled`] : []
    hover(root, (s) => applyCss(root, resolveNodeStyle(ctx.theme, node, ctx.mode, s), ctx), states)
  },
  Link(node, _ctx, root) {
    root.textContent = text(node.props.label) || text(node.props.href)
    root.style.textDecoration = `underline`
  },
  Toggle(node, ctx, root) {
    root.style.cssText += `;display:inline-flex;align-items:center;justify-content:center;cursor:pointer`
    if (node.props.icon) root.appendChild(part(ctx, node, `icon`, `span`)).appendChild(icon(ctx, 16))
    if (node.props.label) root.appendChild(label(ctx, node, `label`, node.props.label)!)
    hover(root, (s) => applyCss(root, resolveNodeStyle(ctx.theme, node, ctx.mode, s), ctx))
  },
  Input(node, ctx, root) {
    field(node, ctx, root, `input`)
  },
  Textarea(node, ctx, root) {
    field(node, ctx, root, `textarea`)
  },
  Select(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column;gap:6px`
    const l = label(ctx, node, `label`, node.props.label)
    if (l) root.appendChild(l)
    const trig = part(ctx, node, `trigger`)
    trig.style.cssText += `;display:flex;align-items:center;justify-content:space-between;gap:8px`
    const options = (node.props.options as { label: string; value: string }[] | undefined) ?? []
    const chosen = options.find((o) => o.value === node.props.value)
    const value = chosen ? el(`span`) : part(ctx, node, `placeholder`, `span`)
    value.textContent = chosen ? chosen.label : text(node.props.placeholder) || `Choose`
    trig.appendChild(value)
    trig.appendChild(icon(ctx, 16))
    root.appendChild(trig)
  },
  DatePicker(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column;gap:6px`
    const l = label(ctx, node, `label`, node.props.label)
    if (l) root.appendChild(l)
    const trig = part(ctx, node, `trigger`)
    trig.style.cssText += `;display:flex;align-items:center;justify-content:space-between;gap:8px`
    trig.appendChild(document.createTextNode(text(node.props.value) || text(node.props.placeholder) || `Pick a date`))
    trig.appendChild(icon(ctx, 16))
    root.appendChild(trig)
  },
  Checkbox(node, ctx, root) {
    root.style.cssText += `;display:flex;align-items:flex-start;gap:8px;cursor:pointer`
    const box = part(ctx, node, `box`)
    box.style.cssText += `;display:flex;align-items:center;justify-content:center;flex-shrink:0`
    if (node.props.checked) box.appendChild(part(ctx, node, `check`, `span`)).appendChild(icon(ctx, 12))
    root.appendChild(box)
    const col = el(`div`)
    col.style.cssText = `display:flex;flex-direction:column;gap:2px`
    const l = label(ctx, node, `label`, node.props.label)
    if (l) col.appendChild(l)
    const d = label(ctx, node, `description`, node.props.description)
    if (d) col.appendChild(d)
    root.appendChild(col)
  },
  Radio(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:${node.props.orientation === `horizontal` ? `row` : `column`}`
    const options = (node.props.options as { label: string; value: string }[] | undefined) ?? []
    for (const o of options) {
      const checked = o.value === node.props.value
      const row = el(`label`)
      row.style.cssText = `display:flex;align-items:center;gap:8px;cursor:pointer`
      const item = part(ctx, node, `item`, `span`, checked ? [`checked`] : [])
      item.style.cssText += `;display:flex;align-items:center;justify-content:center;flex-shrink:0`
      if (checked) item.appendChild(part(ctx, node, `dot`, `span`, [`checked`]))
      row.appendChild(item)
      row.appendChild(label(ctx, node, `label`, o.label)!)
      root.appendChild(row)
    }
  },
  Switch(node, ctx, root) {
    root.style.cssText += `;display:flex;align-items:center;gap:8px;cursor:pointer`
    const track = part(ctx, node, `track`)
    track.style.cssText += `;display:flex;align-items:center;justify-content:${node.props.checked ? `flex-end` : `flex-start`};padding:2px;box-sizing:border-box;flex-shrink:0`
    track.appendChild(part(ctx, node, `thumb`, `span`))
    root.appendChild(track)
    const col = el(`div`)
    col.style.cssText = `display:flex;flex-direction:column;gap:2px`
    const l = label(ctx, node, `label`, node.props.label)
    if (l) col.appendChild(l)
    const d = label(ctx, node, `description`, node.props.description)
    if (d) col.appendChild(d)
    root.appendChild(col)
  },
  Slider(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column;gap:8px`
    const head = el(`div`)
    head.style.cssText = `display:flex;justify-content:space-between`
    const l = label(ctx, node, `label`, node.props.label)
    if (l) head.appendChild(l)
    const v = label(ctx, node, `value`, node.props.value)
    if (v) head.appendChild(v)
    if (head.childNodes.length) root.appendChild(head)
    const track = part(ctx, node, `track`)
    track.style.cssText += `;position:relative;display:flex;align-items:center`
    const min = Number(node.props.min ?? 0)
    const max = Number(node.props.max ?? 100)
    const pct = Math.max(0, Math.min(1, (Number(node.props.value ?? 50) - min) / (max - min || 1)))
    const range = part(ctx, node, `range`)
    range.style.cssText += `;height:100%;width:${pct * 100}%;border-radius:inherit`
    track.appendChild(range)
    const thumb = part(ctx, node, `thumb`, `span`)
    thumb.style.cssText += `;position:absolute;left:calc(${pct * 100}% - 8px)`
    track.appendChild(thumb)
    root.appendChild(track)
  },
  Composer(node, ctx, root) {
    root.style.cssText += `;display:flex;flex-direction:column`
    const f = part(ctx, node, `field`)
    f.style.cssText += `;display:flex;align-items:center`
    const ph = part(ctx, node, `placeholder`, `span`)
    ph.textContent = text(node.props.placeholder) || `Message`
    f.appendChild(ph)
    root.appendChild(f)
    const bar = el(`div`)
    bar.style.cssText = `display:flex;justify-content:flex-end;align-items:center;gap:6px`
    if (node.props.attachments) bar.appendChild(part(ctx, node, `attachment`, `span`)).textContent = `clip.png`
    bar.appendChild(part(ctx, node, `send`, `button`)).appendChild(icon(ctx, 16))
    root.appendChild(bar)
  },
  TreeGuides(node, ctx, root) {
    root.style.cssText += `;display:flex;width:16px;align-self:stretch;justify-content:center;min-height:24px`
    const line = part(ctx, node, `line`)
    line.style.cssText += `;align-self:stretch;background:currentColor`
    root.appendChild(line)
  },
  Unknown(node, ctx, root) {
    const l = part(ctx, node, `label`)
    l.textContent = `Unknown component ${text(node.props.component)}`
    root.appendChild(l)
  },
}

function field(node: UiNode, ctx: PaintContext, root: HTMLElement, tag: `input` | `textarea`): void {
  root.style.cssText += `;display:flex;flex-direction:column;gap:6px`
  const l = label(ctx, node, `label`, node.props.label)
  if (l) root.appendChild(l)
  const f = part(ctx, node, `field`, `div`, node.props.disabled ? [`disabled`] : [])
  f.style.cssText += `;display:flex;align-items:${tag === `textarea` ? `flex-start` : `center`};box-sizing:border-box`
  if (node.props.value) f.textContent = text(node.props.value)
  else {
    const ph = part(ctx, node, `placeholder`, `span`)
    ph.textContent = text(node.props.placeholder) || ``
    f.appendChild(ph)
  }
  root.appendChild(f)
  const d = label(ctx, node, `description`, node.props.description)
  if (d) root.appendChild(d)
}

function overlayPreview(node: UiNode, ctx: PaintContext, root: HTMLElement, kind: string): void {
  root.style.cssText += `;display:flex;flex-direction:column;gap:6px;align-items:flex-start`
  if (node.slots?.trigger) root.appendChild(paint(node.slots.trigger, ctx))
  const c = part(ctx, node, `content`)
  c.style.cssText += `;display:flex;flex-direction:column;max-width:420px`
  const t = label(ctx, node, `title`, node.props.title)
  if (t) c.appendChild(t)
  const d = label(ctx, node, `description`, node.props.description)
  if (d) c.appendChild(d)
  const body = el(`div`)
  body.style.cssText = `display:flex;flex-direction:column;gap:12px`
  paintChildren(ctx, node, body)
  c.appendChild(body)
  if (node.slots?.footer) {
    const f = part(ctx, node, `footer`)
    f.style.cssText += `;display:flex;justify-content:flex-end`
    f.appendChild(paint(node.slots.footer, ctx))
    c.appendChild(f)
  }
  c.dataset.overlay = kind
  root.appendChild(c)
}

const TAGS: Record<string, keyof HTMLElementTagNameMap> = { Button: `button`, Link: `a`, Toggle: `button`, Text: `span`, Link_: `a` }

/** One node → one element (its children inside). */
export function paint(node: UiNode, ctx: PaintContext): HTMLElement {
  const root = el(TAGS[node.component] ?? `div`)
  root.dataset.id = node.id
  root.dataset.component = node.component
  if (node.recipe) root.dataset.recipe = `${node.recipe.macro}/${node.recipe.part}`
  if (root instanceof HTMLButtonElement) root.type = `button`
  root.style.boxSizing = `border-box`
  applyCss(root, resolveNodeStyle(ctx.theme, node, ctx.mode, ctx.states ?? []), ctx)
  const painter = paintNative[node.component]
  if (painter) painter(node, ctx, root)
  else {
    root.textContent = node.component
    paintChildren(ctx, node, root)
  }
  return root
}
