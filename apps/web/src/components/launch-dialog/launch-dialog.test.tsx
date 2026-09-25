import { act, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type { Team } from "@/db/schema"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"

// EXP-1019: the launcher's SHELL. The composer inside it has its own suite
// (`launch-composer.test.tsx`) — this only proves the three things the shell
// owns: it opens on a request, it hands the seed to the composer once, and it
// gets out of the way when the run it started is under way.

const steerEnabled = vi.hoisted(() => ({ value: true }))
const remoteState = vi.hoisted(() => ({ sentTo: null as string | null }))
const composerCalls = vi.hoisted(
  () => [] as { teamId: string; seed: unknown }[]
)

vi.mock(`@/components/agent-session`, () => ({
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
vi.mock(`@/hooks/use-remote-start`, () => ({
  useRemoteStart: () => ({ sentTo: remoteState.sentTo, starting: false }),
}))
vi.mock(`@/hooks/use-launch-composer`, () => ({
  useLaunchComposer: (args: { teamId: string; seed: unknown }) => {
    composerCalls.push({ teamId: args.teamId, seed: args.seed })
    return {
      subject: { kind: `action`, id: `a1`, inputs: {} },
      selectedAction: { id: `a1`, name: `Release train` },
      checkedIssues: [],
      toggleIssue: vi.fn(),
      clearAction: vi.fn(),
      busy: false,
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
  remoteState.sentTo = null
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
    act(() => requestLaunchDialog({ issueIds: [], actionId: `a1` }))
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    expect(screen.getByTestId(`agent-composer`)).toBeTruthy()
    // The accessible name is the headline, not a generic title.
    expect(screen.getByText(`Run Release train`)).toBeTruthy()
    expect(composerCalls[0]?.seed).toEqual({ issueIds: [], actionId: `a1` })
  })

  it(`shuts once the started run is under way, not at send time`, () => {
    const { rerender } = render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    // Sent: the watch that opens the run lives in the dialog, so it stays.
    remoteState.sentTo = `buildbox`
    rerender(<LaunchDialogHost team={team} />)
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    // Cleared: the run's row landed (or the deadline passed) — done here.
    act(() => {
      remoteState.sentTo = null
      rerender(<LaunchDialogHost team={team} />)
    })
    expect(screen.queryByTestId(`launch-dialog`)).toBeNull()
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
