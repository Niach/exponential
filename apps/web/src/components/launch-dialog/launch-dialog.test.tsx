import { act, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type { Team } from "@/db/schema"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"

// EXP-1019: the launcher's SHELL. The composer inside it has its own suite
// (`launch-composer.test.tsx`); this only proves the things the shell owns:
// it opens on a request, it hands the seed to the composer once, it gets out
// of the way once the session STARTED (EXP-1293: the composer then opens its
// thread) and not before: while the start is in flight (`busy`) a
// dismissal is refused; a failed start leaves it open.

const steerEnabled = vi.hoisted(() => ({ value: true }))
const composerState = vi.hoisted(() => ({
  busy: false,
  onStarted: undefined as ((sessionId: string) => void) | undefined,
}))
const remoteCalls = vi.hoisted(() => [] as Record<string, unknown>[])
const composerCalls = vi.hoisted(
  () => [] as { teamId: string; seed: unknown }[]
)

vi.mock(`@/hooks/use-steer-config`, () => ({
  useSteerConfig: () => ({ enabled: steerEnabled.value }),
}))
vi.mock(`@/hooks/use-team-permissions`, () => ({
  useTeamPermissions: () => ({ isMember: true }),
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `u1` } } }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamUsers: () => ({ users: [] }),
}))
vi.mock(`@/hooks/use-steer-devices`, () => ({
  useSteerDevices: (options: Record<string, unknown>) => {
    remoteCalls.push(options)
    return { devices: [], refresh: () => {}, latestVersions: null }
  },
}))
vi.mock(`@/hooks/use-launch-composer`, () => ({
  useLaunchComposer: (args: {
    teamId: string
    seed: unknown
    onStarted?: (sessionId: string) => void
  }) => {
    composerCalls.push({ teamId: args.teamId, seed: args.seed })
    composerState.onStarted = args.onStarted
    return {
      subject: { kind: `issues`, ids: [`i1`] },
      selectedPrompt: null,
      checkedIssues: [{ id: `i1`, identifier: `APP-3` }],
      toggleIssue: vi.fn(),
      clearPrompt: vi.fn(),
      busy: composerState.busy,
    } as unknown as LaunchComposerModel
  },
}))
// The composer is stubbed: the shell's job is to mount it, not to redraw it.
vi.mock(`@/components/launch-composer`, () => ({
  LaunchComposer: () => <div data-testid="agent-composer" />,
}))

import { LaunchDialogHost } from "@/components/launch-dialog/launch-dialog"
import {
  closeLaunchDialog,
  requestLaunchDialog,
} from "@/lib/launch-dialog-store"

const team = { id: `t1`, slug: `acme`, name: `Acme` } as unknown as Team

beforeEach(() => {
  closeLaunchDialog()
  composerCalls.length = 0
  remoteCalls.length = 0
  composerState.busy = false
  composerState.onStarted = undefined
  steerEnabled.value = true
})

describe(`LaunchDialogHost`, () => {
  it(`stays shut until something asks for the launcher`, () => {
    render(<LaunchDialogHost team={team} />)
    expect(screen.queryByTestId(`launch-dialog`)).toBeNull()
    expect(composerCalls.length).toBe(0)
  })

  it(`opens on the seed, names the run, and hands the seed to the composer`, () => {
    render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    expect(screen.getByTestId(`agent-composer`)).toBeTruthy()
    // The accessible name is the headline, not a generic title.
    expect(screen.getByText(`Implement APP-3`)).toBeTruthy()
    expect(composerCalls[0]?.seed).toEqual({ issueIds: [`i1`] })
  })

  it(`shuts once the session started, not at send time`, () => {
    const { rerender } = render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    composerState.busy = true
    rerender(<LaunchDialogHost team={team} />)
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    act(() => {
      composerState.busy = false
      composerState.onStarted?.(`s-new`)
    })
    expect(screen.queryByTestId(`launch-dialog`)).toBeNull()
  })

  it(`refuses a dismissal while the start is in flight, and stays after a failure`, () => {
    const { rerender } = render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    act(() => {
      composerState.busy = true
      rerender(<LaunchDialogHost team={team} />)
    })
    fireEvent.keyDown(document.activeElement ?? document.body, { key: `Escape` })
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    fireEvent.click(screen.getByRole(`button`, { name: `Close` }))
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    // A failed start returns the composer to idle: the dialog stays, and
    // dismisses freely again.
    act(() => {
      composerState.busy = false
      rerender(<LaunchDialogHost team={team} />)
    })
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    fireEvent.click(screen.getByRole(`button`, { name: `Close` }))
    expect(screen.queryByTestId(`launch-dialog`)).toBeNull()
  })

  it(`dismisses freely while nothing has been sent`, () => {
    render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    fireEvent.keyDown(document.activeElement ?? document.body, { key: `Escape` })
    expect(screen.queryByTestId(`launch-dialog`)).toBeNull()
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    fireEvent.click(screen.getByRole(`button`, { name: `Close` }))
    expect(screen.queryByTestId(`launch-dialog`)).toBeNull()
  })

  it(`hands the request to the composer as its seed`, () => {
    render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`], deviceId: `d1` }))
    expect(remoteCalls[0]).toMatchObject({ teamId: `t1` })
    expect(composerCalls[0]?.seed).toEqual({ issueIds: [`i1`], deviceId: `d1` })
  })

  it(`says so instead of doing nothing when the instance has no relay`, () => {
    steerEnabled.value = false
    render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    expect(screen.getByTestId(`launch-dialog`).textContent).toContain(
      `Live steering is unavailable`
    )
    expect(composerCalls.length).toBe(0)
  })
})
