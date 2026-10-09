import { fireEvent, render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { reduceSurface, type FlatComponent } from "@exponential-at/ui"
import { ExponentialSurface } from "@exponential-at/ui-react"
import { appExtensionCatalog, appReactExtension } from "./exponential-ui-app-extension"

// VAPP-102: the app's components paint the extension's row natives.
const paint = (components: FlatComponent[], onAction = vi.fn()) => {
  const { root, templates, issues } = reduceSurface(components, {
    catalogId: appExtensionCatalog.id,
    extensions: [appExtensionCatalog],
  })
  expect(issues).toEqual([])
  const view = render(
    <ExponentialSurface root={root} templates={templates} extensions={[appReactExtension]} host={{ onAction }} theme="exponential" mode="dark" />
  )
  return { ...view, onAction }
}

describe(`the app extension painters`, () => {
  it(`SessionRow is the app's session row and emits press`, () => {
    const { container, onAction } = paint([
      { id: `root`, component: `SessionRow`, title: `Batch runs`, identifier: `EXP-874`, caption: `In review`, agent: `claude`, size: `big`, on: { press: { event: { name: `open` } } } } as FlatComponent,
    ])
    const row = container.querySelector(`[data-session-row="big"]`)!
    expect(row.textContent).toContain(`EXP-874`)
    expect(row.textContent).toContain(`In review`)
    fireEvent.click(row)
    expect(onAction).toHaveBeenCalled()
  })

  it(`StackRail = its PrRows over the base-branch row`, () => {
    const { container } = paint([
      { id: `root`, component: `StackRail`, baseBranch: `master`, children: [`a`, `b`] } as FlatComponent,
      { id: `a`, component: `PrRow`, identifier: `EXP-2`, title: `Top`, word: `stack`, rail: { below: true } } as FlatComponent,
      { id: `b`, component: `PrRow`, identifier: `EXP-1`, title: `Bottom`, state: `current`, rail: { above: true, below: true } } as FlatComponent,
    ])
    const rows = Array.from(container.querySelectorAll(`[data-pr-row]`)).map((r) => r.getAttribute(`data-pr-row`))
    expect(rows).toEqual([`open`, `current`, `base`])
    expect(container.textContent).toContain(`master`)
  })

  it(`RunStatusRow, DiffCounts and DiffFileRow paint the app's atoms`, () => {
    const { container, getByText } = paint([
      { id: `root`, component: `Stack`, children: [`run`, `counts`, `file`] } as FlatComponent,
      { id: `run`, component: `RunStatusRow`, caption: `Working`, toolLine: `Edit a.ts`, tone: `emerald` } as FlatComponent,
      { id: `counts`, component: `DiffCounts`, additions: 12, deletions: 2 } as FlatComponent,
      { id: `file`, component: `DiffFileRow`, path: `src/a.ts`, additions: 1, deletions: 0, status: `deleted` } as FlatComponent,
    ])
    expect(container.querySelector(`[data-testid="run-status-row"]`)!.textContent).toContain(`Edit a.ts`)
    expect(getByText(`+12`)).toBeTruthy()
    expect(container.querySelector(`[data-diff-file-row="removed"]`)!.textContent).toContain(`a.ts`)
  })
})
