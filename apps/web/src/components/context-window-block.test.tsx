import { render, screen, fireEvent } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { ContextSegment } from "@/lib/context-layout"

// EXP-1051: the usage overlay's context block — the headline and the bar at
// rest, the LAYOUT legend behind the chevron, and the two legend rows that are
// links rather than facts (the playbook, the team prompt). The folding itself
// is `lib/context-layout.ts`'s job and its fixture's; this proves the block
// draws what that returns and that the two actions work.

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({
    children,
    to,
    params,
    ...rest
  }: {
    children: React.ReactNode
    to: string
    params?: Record<string, string>
  }) => (
    <a href={to} data-params={JSON.stringify(params ?? {})} {...rest}>
      {children}
    </a>
  ),
}))

vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamById: (teamId: string | null | undefined) =>
    teamId === `t1` ? { id: `t1`, slug: `acme` } : null,
}))

// TipTap does not render under jsdom, and the playbook's BYTES are gated by
// the generator, not here — the dialog only has to show them.
vi.mock(`@/components/issue-editor/markdown-editor`, () => ({
  MarkdownEditor: ({ markdown }: { markdown: string }) => (
    <div data-testid="playbook-markdown">{markdown}</div>
  ),
}))

import { ContextWindowBlock, PLAYBOOK_DIALOG_TITLE } from "@/components/context-window-block"
import { RUN_PLAYBOOK } from "@/lib/run-playbook.generated"

// The contract fixture's first case, as the device would publish it.
const usage = { contextUsed: 65_000, contextSize: 200_000 } as never
const segments: ContextSegment[] = [
  { key: `base`, tokens: 21_000, source: `measured` },
  { key: `tools`, tokens: 2_400, source: `estimated` },
  { key: `playbook`, tokens: 1_500, source: `estimated` },
  { key: `team`, tokens: 800, source: `estimated` },
  {
    key: `project`,
    tokens: 9_800,
    source: `estimated`,
    detail: `CLAUDE.md, ~/.claude/CLAUDE.md`,
  },
  { key: `task`, tokens: 600, source: `estimated` },
]

const draw = (over: Partial<Parameters<typeof ContextWindowBlock>[0]> = {}) =>
  render(
    <ContextWindowBlock
      sessionUsage={usage}
      contextLayout={segments}
      teamId="t1"
      {...over}
    />
  )

const legendKeys = () =>
  screen
    .queryAllByTestId(`context-legend-row`)
    .map((node) => node.dataset.key)

describe(`ContextWindowBlock`, () => {
  it(`draws nothing until the engine reports a window`, () => {
    const { container } = render(
      <ContextWindowBlock sessionUsage={null} contextLayout={segments} teamId="t1" />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`shows the headline, the cost and one bar slice per layer, folded`, () => {
    const { container } = draw({ cost: `$0.42` })
    expect(screen.getByTestId(`context-window-block`)).toBeTruthy()
    expect(screen.getByText(`65k / 200k (32%)`)).toBeTruthy()
    expect(screen.getByText(`$0.42`)).toBeTruthy()
    expect(
      Array.from(
        container.querySelectorAll<HTMLElement>(
          `[data-slot="segmented-bar-segment"]`
        )
      ).map((node) => node.dataset.key)
    ).toEqual([`base`, `tools`, `playbook`, `team`, `project`, `task`, `conversation`])
    // The legend is behind the chevron.
    expect(legendKeys()).toEqual([])
    expect(
      screen.getByTestId(`context-window-toggle`).getAttribute(`aria-expanded`)
    ).toBe(`false`)
  })

  it(`opens the legend on the toggle, Free last`, () => {
    draw()
    fireEvent.click(screen.getByTestId(`context-window-toggle`))
    expect(
      screen.getByTestId(`context-window-toggle`).getAttribute(`aria-expanded`)
    ).toBe(`true`)
    expect(legendKeys()).toEqual([
      `base`,
      `tools`,
      `playbook`,
      `team`,
      `project`,
      `task`,
      `conversation`,
      `free`,
    ])
    // The labels, tokens and shares the fixture pins.
    expect(screen.getByText(`Base`)).toBeTruthy()
    expect(screen.getByText(`Team prompt`)).toBeTruthy()
    expect(screen.getByText(`Free`)).toBeTruthy()
    expect(screen.getByText(`10.5%`)).toBeTruthy()
    // An estimate is marked as one; a measured layer is not.
    expect(screen.getByText(`≈2.4k`)).toBeTruthy()
    expect(screen.getByText(`21k`)).toBeTruthy()
    // The project row names its files rather than linking anywhere.
    expect(screen.getByText(`CLAUDE.md, ~/.claude/CLAUDE.md`)).toBeTruthy()
    expect(
      screen.getByText(`CLAUDE.md, ~/.claude/CLAUDE.md`).closest(`a,button`)
    ).toBeNull()
  })

  it(`opens the run playbook from its legend row`, () => {
    draw()
    fireEvent.click(screen.getByTestId(`context-window-toggle`))
    expect(screen.queryByTestId(`playbook-markdown`)).toBeNull()
    const row = screen
      .getAllByTestId(`context-legend-row`)
      .find((node) => node.dataset.key === `playbook`)
    expect(row?.tagName).toBe(`BUTTON`)
    fireEvent.click(row as HTMLElement)
    expect(screen.getByText(PLAYBOOK_DIALOG_TITLE)).toBeTruthy()
    expect(screen.getByTestId(`playbook-markdown`).textContent).toBe(RUN_PLAYBOOK)
  })

  it(`links the team-prompt row into the team's general settings`, () => {
    draw()
    fireEvent.click(screen.getByTestId(`context-window-toggle`))
    const row = screen
      .getAllByTestId(`context-legend-row`)
      .find((node) => node.dataset.key === `team`)
    expect(row?.tagName).toBe(`A`)
    expect(row?.getAttribute(`href`)).toBe(`/t/$teamSlug/settings/general`)
    expect(row?.dataset.params).toBe(JSON.stringify({ teamSlug: `acme` }))
  })

  it(`leaves the team row a plain row while the team has not synced`, () => {
    render(
      <ContextWindowBlock
        sessionUsage={usage}
        contextLayout={segments}
        teamId={null}
      />
    )
    fireEvent.click(screen.getByTestId(`context-window-toggle`))
    const row = screen
      .getAllByTestId(`context-legend-row`)
      .find((node) => node.dataset.key === `team`)
    expect(row?.tagName).toBe(`DIV`)
  })
})
