// VAPP-87: the surface's behaviour — host-owned inputs with revisions,
// optimistic actions, bindings and templates through useSurface, two
// themes on one page, the Unknown placeholder, the windowed list.

import { describe, expect, it, vi } from "vitest"
import { act, fireEvent, render, renderHook } from "@testing-library/react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, THEME_SCHEMA_ID, reduceNested, builtinTheme } from "@exponential-at/ui"
import type { NestedNode, ThemeIssue, ThemeSource } from "@exponential-at/ui"
import { ExponentialSurface, resolveThemeInput } from "./surface"
import { useSurface } from "./use-surface"
import { useHostOwnedValue } from "./inputs"
import { compileTheme } from "./theme-css"
import { WINDOW_THRESHOLD } from "./natives/layout"
import type { HostPlugin, SurfaceActionEvent, SurfaceInputEvent } from "./host"

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root

describe(`host-owned value`, () => {
  it(`debounces to one change, commits at once, applies an echo only when idle and acked`, async () => {
    vi.useFakeTimers()
    const sent: [unknown, number, string][] = []
    let resolveAck: (() => void) | null = null
    const send = vi.fn((value: unknown, revision: number, kind: string) => {
      sent.push([value, revision, kind])
      return new Promise<void>((r) => (resolveAck = r))
    })
    let external = ``
    const { result, rerender } = renderHook(() => useHostOwnedValue<string>({ external, send }))
    act(() => result.current.onFocus())
    act(() => result.current.edit(`a`))
    act(() => result.current.edit(`ab`))
    act(() => result.current.edit(`abc`))
    expect(result.current.value).toBe(`abc`)
    expect(result.current.pending).toBe(true)
    expect(sent).toEqual([])
    act(() => vi.advanceTimersByTime(160))
    expect(sent).toEqual([[`abc`, 3, `change`]])
    // A stale echo while pending: ignored (focused AND unacked).
    external = `ab`
    rerender()
    expect(result.current.value).toBe(`abc`)
    // Ack, blur → commit, then the host's push with the final value lands.
    await act(async () => {
      resolveAck!()
      await Promise.resolve()
    })
    act(() => result.current.onBlur())
    expect(sent[1]).toEqual([`abc`, 3, `commit`])
    await act(async () => {
      resolveAck!()
      await Promise.resolve()
    })
    external = `abc!`
    rerender()
    expect(result.current.value).toBe(`abc!`)
    vi.useRealTimers()
  })
})

describe(`inputs in a surface`, () => {
  it(`Input sends {name, path, value, revision} and writes through to the data model`, () => {
    vi.useFakeTimers()
    const inputs: SurfaceInputEvent[] = []
    const host: HostPlugin = { onInput: (e) => void inputs.push(e) }
    const root = tree({ id: `root`, component: `Input`, props: { label: `Title`, name: `title`, value: { path: `/draft/title` } } })
    const { container } = render(<ExponentialSurface root={root} data={{ draft: { title: `` } }} host={host} theme="neutral" id="in" />)
    const input = container.querySelector(`input`)!
    fireEvent.focus(input)
    fireEvent.change(input, { target: { value: `He` } })
    fireEvent.change(input, { target: { value: `Hey` } })
    expect(input.value).toBe(`Hey`)
    act(() => vi.advanceTimersByTime(200))
    expect(inputs).toEqual([{ surfaceId: `in`, componentId: `root`, name: `title`, path: `/draft/title`, value: `Hey`, revision: 2, kind: `change` }])
    fireEvent.blur(input)
    expect(inputs[1].kind).toBe(`commit`)
    vi.useRealTimers()
  })
})

describe(`actions`, () => {
  it(`a Button press reports the action and stays pending until the host settles`, async () => {
    let resolve: (() => void) | null = null
    const actions: SurfaceActionEvent[] = []
    const host: HostPlugin = { onAction: (e) => (actions.push(e), new Promise<void>((r) => (resolve = r))) }
    const root = tree({ id: `root`, component: `Button`, props: { label: `Scan` }, on: { press: { event: { name: `scan`, context: { source: { path: `/src` } } } } } })
    const { container } = render(<ExponentialSurface root={root} data={{ src: `reddit` }} host={host} theme="neutral" id="act" />)
    const button = container.querySelector(`button`)!
    expect(button.disabled).toBe(false)
    fireEvent.click(button)
    expect(actions[0]).toMatchObject({ surfaceId: `act`, event: `press`, name: `scan`, componentId: `root`, context: { source: `reddit` } })
    expect(button.disabled).toBe(true)
    expect(button.getAttribute(`aria-busy`)).toBe(`true`)
    await act(async () => {
      resolve!()
      await Promise.resolve()
    })
    expect(button.disabled).toBe(false)
  })
  it(`a macro part's routed event (Pagination → change) reaches the host with its context`, () => {
    const actions: SurfaceActionEvent[] = []
    const host: HostPlugin = { onAction: (e) => void actions.push(e) }
    const root = tree({ id: `pg`, component: `Pagination`, props: { page: 2, totalPages: 5 }, on: { change: { event: { name: `page` } } } })
    const { container } = render(<ExponentialSurface root={root} host={host} theme="neutral" id="pg" />)
    const next = container.querySelector(`[data-xui-id="pg.next"]`) as HTMLButtonElement
    fireEvent.click(next)
    expect(actions[0]).toMatchObject({ name: `page`, event: `press`, componentId: `pg.next`, context: { page: 3 } })
  })
})

describe(`useSurface`, () => {
  it(`applies A2UI messages, resolves bindings and renders a template per item`, () => {
    const { result } = renderHook(() => useSurface({ surfaceId: `s` }))
    act(() => {
      result.current.apply({ version: `v0.9`, createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG_ID } })
      result.current.apply({
        version: `v0.9`,
        updateComponents: {
          surfaceId: `s`,
          components: [
            { id: `root`, component: `List`, children: { componentId: `row`, path: `/rows` } },
            { id: `row`, component: `Text`, text: { path: `title` } },
          ],
        },
      })
      result.current.apply({ version: `v0.9`, updateDataModel: { surfaceId: `s`, path: `/rows`, value: [{ title: `one` }, { title: `two` }] } })
    })
    expect(result.current.root?.component).toBe(`List`)
    expect(result.current.root?.template).toEqual({ component: `row`, path: `/rows` })
    const { container } = render(<ExponentialSurface surface={result.current} theme="neutral" />)
    const texts = Array.from(container.querySelectorAll(`[data-xui-c="Text"]`)).map((e) => e.textContent)
    expect(texts).toEqual([`one`, `two`])
    expect(container.querySelector(`[data-xui-id="row.1"]`)).not.toBeNull()
    act(() => result.current.apply({ version: `v0.9`, deleteSurface: { surfaceId: `s` } }))
    expect(result.current.root).toBeNull()
    expect(result.current.deleted).toBe(true)
  })
  it(`ignores messages for another surface`, () => {
    const { result } = renderHook(() => useSurface({ surfaceId: `s` }))
    act(() => result.current.apply({ updateDataModel: { surfaceId: `other`, path: `/x`, value: 1 } }))
    expect(result.current.data).toEqual({})
  })
})

describe(`themes`, () => {
  it(`two surfaces on one page wear two themes, each with its own scope and mode`, () => {
    const root = tree({ id: `root`, component: `Button`, props: { label: `A` } })
    const { container } = render(
      <div>
        <ExponentialSurface root={root} theme="neutral" mode="light" id="one" />
        <ExponentialSurface root={root} theme="playful" mode="dark" id="two" />
      </div>
    )
    const [a, b] = Array.from(container.querySelectorAll(`.xui-surface`)) as HTMLElement[]
    expect(a.classList.contains(compileTheme(builtinTheme(`neutral`)).scope)).toBe(true)
    expect(b.classList.contains(compileTheme(builtinTheme(`playful`)).scope)).toBe(true)
    expect(a.dataset.xuiMode).toBe(`light`)
    expect(b.dataset.xuiMode).toBe(`dark`)
    // Every painted element carries its own theme's scope, never the other's.
    const aButton = a.querySelector(`[data-xui-c="Button"]`)!
    expect(aButton.classList.contains(compileTheme(builtinTheme(`neutral`)).scope)).toBe(true)
    expect(aButton.classList.contains(compileTheme(builtinTheme(`playful`)).scope)).toBe(false)
  })
  it(`a host theme file resolves against the built-ins`, () => {
    const root = tree({ id: `root`, component: `Text`, props: { text: `hi` } })
    const { container } = render(<ExponentialSurface root={root} theme={{ $schema: THEME_SCHEMA_ID, id: `brand`, name: `Brand`, extends: `neutral`, modes: { light: { color: { primary: `#2563eb` } } } }} mode="light" id="brand" />)
    expect(container.querySelector(`style[data-xui-style="theme"]`)!.textContent).toContain(`--xui-color-primary:#2563eb`)
  })
  it(`round 4: an unusable theme never crashes the host; it paints the default theme and reports the issues`, () => {
    const root = tree({ id: `root`, component: `Text`, props: { text: `hi` } })
    const seen: ThemeIssue[][] = []
    const host = { onThemeIssues: (issues: ThemeIssue[]) => seen.push(issues) }
    const { container } = render(
      <>
        <ExponentialSurface root={root} theme="nope" host={host} id="unknown-id" />
        <ExponentialSurface root={root} theme={{ id: `brand`, name: `Brand`, extends: `neutral` } as unknown as ThemeSource} host={host} id="no-schema" />
        <ExponentialSurface root={root} theme={{ $schema: THEME_SCHEMA_ID, id: `bad`, name: `Bad`, extends: `neutral`, tokens: { spacing: { huge: 1 } } } as unknown as ThemeSource} host={host} id="bad-token" />
      </>
    )
    expect(container.querySelectorAll(`[data-xui-theme="${DEFAULT_THEME_ID}"]`).length).toBe(3)
    expect(seen.map((issues) => issues[0]!.path)).toEqual([`theme`, `$schema`, `tokens.spacing.huge`])
    expect(resolveThemeInput(`nope`).theme.id).toBe(DEFAULT_THEME_ID)
  })
})

describe(`placeholders and lists`, () => {
  it(`an unknown component paints the placeholder and tells the host`, () => {
    const seen: string[] = []
    const root = reduceNested({ id: `root`, component: `Gauge` as never, props: {} }, { catalogId: CORE_CATALOG_ID }).root
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="u" host={{ onUnknown: (n) => void seen.push(String(n.props.component)) }} />)
    expect(container.querySelector(`[data-xui-c="Unknown"]`)!.textContent).toContain(`Unknown component Gauge`)
    expect(seen).toEqual([`Gauge`])
  })
  it(`a long List windows its rows`, () => {
    const children = Array.from({ length: WINDOW_THRESHOLD + 20 }, (_, i) => ({ id: `r${i}`, component: `Text`, props: { text: `row ${i}` } }))
    const root = tree({ id: `root`, component: `List`, props: { divided: true }, children })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="l" />)
    const window = container.querySelector(`.xui-list-window`) as HTMLElement
    expect(window).not.toBeNull()
    expect(Number(window.dataset.windowStart)).toBe(0)
    expect(Number(window.dataset.windowEnd)).toBeLessThan(children.length)
    expect(container.querySelectorAll(`.xui-list-item`).length).toBe(Number(window.dataset.windowEnd))
  })
})
