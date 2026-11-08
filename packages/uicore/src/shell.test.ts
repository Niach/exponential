import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/shell.json" with { type: "json" }
import {
  altTabOrder,
  hydrate,
  reduce,
  startMenuApps,
  startMenuCategories,
  taskbarButtons,
  taskbarPress,
  taskbarScroll,
  workspaceBadges,
  switcherSections,
  type ShellEvent,
  type ShellState,
  type StartMenuApp,
} from "./shell"

const given = (state: unknown): ShellState => hydrate(state as Partial<ShellState>)

describe(`shell.json reduce`, () => {
  test(`the fixture has at least 60 reduce cases, each named by a sentence`, () => {
    expect(fixture.reduce.length).toBeGreaterThanOrEqual(60)
    for (const c of fixture.reduce) expect(c.name.split(` `).length).toBeGreaterThan(3)
  })

  test(`every event kind has a case`, () => {
    const kinds = new Set(fixture.reduce.map((c) => c.event.kind))
    for (const kind of [
      `rail.toggle`, `sidebar.toggle`, `switcher.pick`, `switcher.set`,
      `window.opened`, `window.closed`, `window.focused`, `window.titled`, `window.minimized`, `window.restored`, `window.moved`,
      `workspace.switched`, `workspace.count`,
      `taskbar.pin`, `taskbar.unpin`, `taskbar.hover`, `taskbar.unhover`, `taskbar.previews`, `taskbar.menu`,
      `startMenu.toggle`, `startMenu.open`, `startMenu.close`, `startMenu.query`, `startMenu.category`,
      `chat.toggle`, `altTab.next`, `altTab.prev`, `altTab.commit`, `altTab.cancel`,
    ]) expect(kinds.has(kind), kind).toBe(true)
  })

  for (const c of fixture.reduce) {
    test(c.name, () => {
      const before = given(c.state)
      const want = { ...given(c.state), ...(c.expected as Partial<ShellState>) }
      expect(reduce(before, c.event as ShellEvent)).toEqual(want)
      expect(before).toEqual(given(c.state))
    })
  }
})

const apps = fixture.apps as StartMenuApp[]
const favourites = fixture.favourites as string[]

describe(`shell.json selectors`, () => {
  for (const c of fixture.selectors.taskbarButtons) test(c.name, () => expect(taskbarButtons(given(c.state))).toEqual(c.expected))
  for (const c of fixture.selectors.taskbarPress) test(c.name, () => expect(taskbarPress(given(c.state), c.args.app)).toEqual(c.expected as ReturnType<typeof taskbarPress>))
  for (const c of fixture.selectors.taskbarScroll) test(c.name, () => expect(taskbarScroll(given(c.state), c.args.app, c.args.direction as `up` | `down`)).toEqual(c.expected as ReturnType<typeof taskbarScroll>))
  for (const c of fixture.selectors.workspaceBadges) test(c.name, () => expect(workspaceBadges(given(c.state))).toEqual(c.expected))
  for (const c of fixture.selectors.switcherSections) test(c.name, () => expect(switcherSections(given(c.state as never))).toEqual(c.expected as ReturnType<typeof switcherSections>))
  for (const c of fixture.selectors.altTabOrder) test(c.name, () => expect(altTabOrder(given(c.state))).toEqual(c.expected))
  for (const c of fixture.selectors.startMenuApps) {
    const args = (c as { args?: { apps?: StartMenuApp[]; favourites?: string[] } }).args ?? {}
    test(c.name, () => expect(startMenuApps(given(c.state), args.apps ?? apps, args.favourites ?? favourites)).toEqual(c.expected))
  }
  for (const c of fixture.selectors.startMenuCategories) {
    const args = (c as { args?: { apps?: StartMenuApp[]; favourites?: string[] } }).args ?? {}
    test(c.name, () => expect(startMenuCategories(args.apps ?? apps, args.favourites ?? favourites)).toEqual(c.expected))
  }
})
