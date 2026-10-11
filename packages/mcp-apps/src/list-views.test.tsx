import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { McpActionsProvider, type McpActions } from "./actions"
import { DevicesView } from "./devices-view"
import { InboxView } from "./inbox-view"
import { IssueListView } from "./issue-list-view"
import type { DeviceRow, IssueRow, NotificationRow, RunDetail } from "./model"
import { RunsListView } from "./runs-list-view"

afterEach(cleanup)

describe(`RunsListView`, () => {
  it(`nests a child run under its parent and opens a row`, () => {
    const runs = [
      { id: `p`, status: `running`, agent: `claude`, agentBusy: true, issueIdentifier: `EXP-1`, issueTitle: `Parent` },
      { id: `c`, status: `running`, agent: `codex`, parentSessionId: `p`, actionName: `Tidy up` },
      { id: `e`, status: `ended`, deviceLabel: `box`, issueIdentifier: `EXP-2`, issueTitle: `Old` },
    ] as RunDetail[]
    const opened: string[] = []
    render(<RunsListView runs={runs} onOpen={(run) => opened.push(run.id)} />)
    expect(screen.getByText(`Running`)).toBeTruthy()
    expect(screen.getByText(`Ended`)).toBeTruthy()
    expect(screen.getByLabelText(`Collapse child runs`)).toBeTruthy()
    fireEvent.click(screen.getByLabelText(`Collapse child runs`))
    expect(screen.queryByText(`Tidy up`)).toBeNull()
    expect(opened).toEqual([])
    fireEvent.click(screen.getByTestId(`run-row-e`))
    expect(opened).toEqual([`e`])
  })
})

describe(`DevicesView`, () => {
  it(`lists a machine's logins; no sign-in without the agent-login cap`, () => {
    const devices = [
      {
        deviceId: `d1`,
        label: `Studio`,
        kind: `desktop`,
        online: true,
        agents: [`claude`],
        agentAccounts: {
          claude: {
            signedIn: false,
            profiles: [{ id: `p1`, signedIn: false, health: `needs_relogin`, email: `me@x.test` }],
          },
        },
      },
      { deviceId: `d2`, label: `Shared box`, kind: `server`, owner: { id: `u`, name: `Ann` }, agentAccounts: {} },
    ] as unknown as DeviceRow[]
    render(<DevicesView devices={devices} />)
    expect(screen.getByText(`My devices`)).toBeTruthy()
    expect(screen.getByText(`Team devices`)).toBeTruthy()
    expect(screen.getByText(`me@x.test`)).toBeTruthy()
    expect(screen.getAllByText(`Needs re-login`).length).toBe(2)
    expect(screen.getByText(`No login reported`)).toBeTruthy()
    expect(screen.queryByText(/add account/i)).toBeNull()
  })
})

describe(`DevicesView sign-in (EXP-1199)`, () => {
  it(`adds an account through the tool, hands the code back and lands`, async () => {
    const before = {
      deviceId: `d1`,
      label: `Studio`,
      kind: `desktop`,
      online: true,
      caps: [`agent-login`],
      agents: [`claude`],
      agentAccounts: { claude: { signedIn: true, email: `me@x.test`, profiles: [{ id: `home`, signedIn: true, active: true }] } },
    }
    const after = {
      ...before,
      agentAccounts: {
        claude: {
          signedIn: true,
          profiles: [
            { id: `home`, signedIn: true, active: true },
            { id: `p2`, signedIn: true, email: `work@x.test`, lastLoginAt: `2026-10-10T19:00:00Z` },
          ],
        },
      },
    }
    const calls: Array<[string, Record<string, unknown>]> = []
    const actions: McpActions = {
      call: (async (name: string, args: Record<string, unknown>) => {
        calls.push([name, args])
        if (name === `exponential_devices_list`) return { kind: `ok`, data: [after] }
        if (args.code) return { kind: `ok`, data: { status: `signing_in`, commandId: `c2` } }
        return { kind: `ok`, data: { status: `url`, commandId: `c1`, url: `https://claude.com/x`, code: null } }
      }) as McpActions[`call`],
      openLink: vi.fn(),
    }
    vi.useFakeTimers({ shouldAdvanceTime: true })
    render(
      <McpActionsProvider value={actions}>
        <DevicesView devices={[before] as unknown as DeviceRow[]} />
      </McpActionsProvider>
    )
    fireEvent.click(screen.getByTestId(`add-account-d1`))
    await waitFor(() => expect(screen.getByText(`Open sign-in page`)).toBeTruthy())
    expect(calls[0]).toEqual([
      `exponential_devices_account_login`,
      { deviceId: `d1`, agent: `claude` },
    ])
    fireEvent.click(screen.getByText(`Open sign-in page`))
    expect(actions.openLink).toHaveBeenCalledWith(`https://claude.com/x`)
    fireEvent.change(screen.getByLabelText(`Code from the browser`), { target: { value: `abc` } })
    fireEvent.click(screen.getByText(`Enter code`))
    await waitFor(() => expect(screen.getByText(`Signing in…`)).toBeTruthy())
    expect(calls[1]).toEqual([`exponential_devices_account_login`, { deviceId: `d1`, agent: `claude`, code: `abc` }])
    await vi.advanceTimersByTimeAsync(5_000)
    await waitFor(() => expect(screen.getByText(`Signed in`)).toBeTruthy())
    expect(screen.getByText(`work@x.test`)).toBeTruthy()
    vi.useRealTimers()
  })

  it(`offers Sign in on a dead credential, re-signing that login`, async () => {
    const device = {
      deviceId: `d1`,
      label: `Studio`,
      online: true,
      caps: [`agent-login`],
      agents: [`codex`],
      agentAccounts: { codex: { signedIn: true, profiles: [{ id: `p9`, signedIn: true, health: `needs_relogin`, email: `w@x.test` }] } },
    }
    const calls: Array<Record<string, unknown>> = []
    const actions: McpActions = {
      call: (async (_name: string, args: Record<string, unknown>) => {
        calls.push(args)
        return { kind: `ok`, data: { status: `url`, commandId: `c1`, url: `https://auth.openai.com/codex/device`, code: `WDJB-MJHT` } }
      }) as McpActions[`call`],
      openLink: () => {},
    }
    render(
      <McpActionsProvider value={actions}>
        <DevicesView devices={[device] as unknown as DeviceRow[]} />
      </McpActionsProvider>
    )
    fireEvent.click(screen.getByTestId(`sign-in-codex:p9`))
    await waitFor(() => expect(screen.getByText(`WDJB-MJHT`)).toBeTruthy())
    expect(calls[0]).toEqual({ deviceId: `d1`, agent: `codex`, profileId: `p9` })
    expect(screen.queryByLabelText(`Code from the browser`)).toBeNull()
  })
})

describe(`IssueListView`, () => {
  it(`resolves assignees through the host`, async () => {
    const calls: string[] = []
    const actions: McpActions = {
      call: (async (name: string) => {
        calls.push(name)
        if (name === `exponential_boards_get`) return { kind: `ok`, data: { teamId: `t1` } }
        return { kind: `ok`, data: [{ id: `u1`, name: `Ada Lovelace` }] }
      }) as McpActions[`call`],
      openLink: () => {},
    }
    const issues = [
      { id: `i1`, identifier: `EXP-1`, title: `One`, status: `backlog`, priority: `high`, boardId: `b1`, assigneeId: `u1`, dueDate: `2020-01-01` },
    ] as unknown as IssueRow[]
    render(
      <McpActionsProvider value={actions}>
        <IssueListView issues={issues} />
      </McpActionsProvider>
    )
    expect(screen.getByText(`Jan 1`)).toBeTruthy()
    await waitFor(() => expect(screen.getByTitle(`Ada Lovelace`)).toBeTruthy())
    expect(calls).toEqual([`exponential_boards_get`, `exponential_members_list`])
  })
})

describe(`InboxView`, () => {
  it(`folds an issue's notifications and opens the issue`, () => {
    const notes = [
      { id: `n1`, issueId: `i1`, type: `issue_comment`, title: `New comment`, body: `Looks good`, readAt: null, createdAt: new Date().toISOString() },
      { id: `n2`, issueId: `i1`, type: `issue_assigned`, title: `Assigned`, readAt: null, createdAt: new Date().toISOString() },
    ] as NotificationRow[]
    const opened: string[] = []
    render(<InboxView notifications={notes} onOpen={(row) => opened.push(row.id)} />)
    expect(screen.getByText(`+1`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`inbox-row-n1`))
    expect(opened).toEqual([`n1`])
  })
})
