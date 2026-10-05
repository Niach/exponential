import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { McpActionsProvider, type McpActions } from "./actions"
import { RunView } from "./run-view"
import type { RunDetail } from "./model"

const PIC = `0b0c6a52-2b0e-4b8e-9d5d-1f9f0e7c1b01`

function harness(answers: Record<string, unknown>) {
  const calls: [string, Record<string, unknown>][] = []
  const actions: McpActions = {
    call: (async (name: string, args: Record<string, unknown>) => {
      calls.push([name, args])
      return { kind: `ok`, data: answers[name] ?? {} }
    }) as McpActions[`call`],
    openLink: vi.fn(),
  }
  return { actions, calls }
}

afterEach(cleanup)

describe(`RunView`, () => {
  it(`resolves pictures to signed URLs and steers a live run`, async () => {
    const run: RunDetail = {
      id: `run-1`,
      status: `running`,
      agent: `claude`,
      agentBusy: true,
      agentCaption: `Editing run-view.tsx`,
      issueId: `issue-1`,
      issueIdentifier: `EXP-1`,
      issueTitle: `Run face`,
      prUrl: `https://github.com/o/r/pull/9`,
      prNumber: 9,
      prState: `open`,
      url: `https://exp.test/t/x/sessions/run-1`,
      results: [
        { topic: `Summary`, text: `Done.` },
        { topic: `Face`, label: `web`, url: `https://exp.test/api/attachments/${PIC}` },
      ],
    }
    const { actions, calls } = harness({
      exponential_attachments_get: { downloadUrl: `https://signed.test/pic.png` },
      exponential_issues_pr_files: {
        files: [{ filename: `a.ts`, status: `modified`, additions: 2, deletions: 1, patch: `@@ -1 +1,2 @@\n-a\n+b\n+c` }],
      },
      exponential_sessions_message: { delivered: true, queued: true },
    })
    render(
      <McpActionsProvider value={actions}>
        <RunView run={run} onOpenLink={() => {}} />
      </McpActionsProvider>
    )
    expect(screen.getByTestId(`run-caption`).textContent).toBe(`Editing run-view.tsx`)
    await waitFor(() => {
      const img = document.querySelector(`img[alt="web"]`)
      expect(img?.getAttribute(`src`)).toBe(`https://signed.test/pic.png`)
    })
    await waitFor(() => expect(screen.getByTestId(`run-changes`).textContent).toContain(`1 file`))
    const box = screen.getByLabelText(`Message the run`)
    fireEvent.change(box, { target: { value: `go on` } })
    await act(async () => {
      fireEvent.keyDown(box, { key: `Enter` })
    })
    expect(calls).toContainEqual([`exponential_sessions_message`, { id: `run-1`, message: `go on` }])
    await waitFor(() => expect(screen.getByRole(`status`).textContent).toMatch(/^Queued/))
  })

  it(`resumes an ended run and switches to the new one`, async () => {
    const { actions, calls } = harness({
      exponential_sessions_start: { sessionId: `run-2` },
      exponential_sessions_get: { id: `run-2`, status: `running`, actionName: `Resumed` },
    })
    render(
      <McpActionsProvider value={actions}>
        <RunView run={{ id: `run-1`, status: `ended`, deviceId: `dev-1` } as RunDetail} />
      </McpActionsProvider>
    )
    await act(async () => {
      fireEvent.click(screen.getByText(`Resume`))
    })
    expect(calls).toContainEqual([
      `exponential_sessions_start`,
      { deviceId: `dev-1`, resumeSessionId: `run-1` },
    ])
    await waitFor(() => expect(screen.getByText(`Resumed`)).toBeTruthy())
    expect(screen.getByLabelText(`Message the run`)).toBeTruthy()
  })
})
