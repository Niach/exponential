import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { DuplicateWarningRow, WorkflowCard } from "@/components/workflow-card"
import type { WorkflowState } from "@/lib/agent-feed"

// EXP-850 §3/§4: the workflow card and the duplicate warning row.

const workflow = (over: Partial<WorkflowState> = {}): WorkflowState => ({
  id: `toolu_w`,
  name: `wire-probe`,
  description: `Probe the workflow progress wire`,
  status: `running`,
  phases: [
    { index: 1, title: `Alpha` },
    { index: 2, title: `Beta` },
  ],
  agents: [
    {
      index: 1,
      label: `alpha:one`,
      phaseIndex: 1,
      agentId: `a1`,
      model: `claude-haiku-4-5`,
      state: `done`,
      tokens: 9_629,
      toolCalls: 3,
      durationMs: 1_075,
      resultPreview: `ok`,
    },
    {
      index: 2,
      label: `beta:shell`,
      phaseIndex: 2,
      agentId: `a2`,
      state: `running`,
      lastToolSummary: `Running cargo test`,
    },
  ],
  ...over,
})

describe(`WorkflowCard`, () => {
  it(`leads with the shared caption, the description and the phase strip`, () => {
    render(<WorkflowCard workflow={workflow()} />)
    expect(
      screen.getByText(`Workflow wire-probe · 1/2 agents done · Beta`)
    ).toBeTruthy()
    expect(screen.getByText(`Probe the workflow progress wire`)).toBeTruthy()
    const phases = screen.getAllByTestId(`workflow-phase`)
    expect(phases).toHaveLength(2)
    expect(phases[0].textContent).toBe(`Alpha1 done`)
    expect(phases[1].textContent).toBe(`Beta1 running`)
  })

  it(`one row per agent, with its telemetry and its note`, () => {
    render(<WorkflowCard workflow={workflow()} />)
    expect(screen.getByText(`alpha:one`)).toBeTruthy()
    expect(
      screen.getByText(`claude-haiku-4-5 · 9.6k tokens · 3 tool calls · 1s`)
    ).toBeTruthy()
    // Done says its result, running says the tool it is on.
    expect(screen.getByText(`ok`)).toBeTruthy()
    expect(screen.getByText(`Running cargo test`)).toBeTruthy()
  })

  it(`a failed agent shows its error, and the summary closes the card`, () => {
    render(
      <WorkflowCard
        workflow={workflow({
          status: `failed`,
          summary: `One lane died`,
          agents: [
            { index: 1, label: `alpha:one`, state: `error`, error: `panicked` },
          ],
        })}
      />
    )
    expect(screen.getByText(`Workflow wire-probe · failed`)).toBeTruthy()
    expect(screen.getByText(`panicked`)).toBeTruthy()
    expect(screen.getByText(`One lane died`)).toBeTruthy()
  })

  it(`an agent with nested events folds them away`, () => {
    render(
      <WorkflowCard
        workflow={workflow()}
        agentEvents={new Map([[`a2`, <span key="x">nested rows</span>]])}
      />
    )
    expect(screen.queryByText(`nested rows`)).toBeNull()
    fireEvent.click(screen.getByRole(`button`, { name: /beta:shell/ }))
    expect(screen.getByText(`nested rows`)).toBeTruthy()
    // The agent WITHOUT nested events has no expander at all.
    expect(screen.queryByRole(`button`, { name: /alpha:one/ })).toBeNull()
  })

  it(`carries the duplicate warnings that named it (§4)`, () => {
    const detail = `Second copy of slowpoke started while the first is still running (resumed by SendMessage)`
    render(<WorkflowCard workflow={workflow()} duplicates={[detail]} />)
    expect(screen.getByTestId(`subagent-duplicate-warning`).textContent).toBe(
      detail
    )
  })
})

describe(`DuplicateWarningRow`, () => {
  it(`renders the wire sentence verbatim`, () => {
    const detail = `Second copy of lane-3 started while the first is still running (resumed by SendMessage)`
    render(<DuplicateWarningRow detail={detail} />)
    expect(screen.getByTestId(`subagent-duplicate-warning`).textContent).toBe(
      detail
    )
  })
})
