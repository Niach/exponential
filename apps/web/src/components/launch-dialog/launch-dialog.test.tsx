import { act, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type { Team } from "@/db/schema"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"

// EXP-1019: the launcher's SHELL. The composer inside it has its own suite
// (`launch-composer.test.tsx`) — this only proves the things the shell owns:
// it opens on a request, it hands the seed to the composer once, it gets out
// of the way when the run it started LANDS — and NOT before: while a start is
// pending (the composer's `starting`, EXP-1271) the watch and the deadline
// live in its hooks, so a dismissal is refused; a timeout leaves it open.
// EXP-870: the request's origin reaches the watch.

const steerEnabled = vi.hoisted(() => ({ value: true }))
const composerState = vi.hoisted(() => ({
  starting: null as { deviceLabel: string; since: number } | null,
  onLanded: undefined as (() => void) | undefined,
}))
const remoteCalls = vi.hoisted(() => [] as Record<string, unknown>[])
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
  useRemoteStart: (options: Record<string, unknown>) => {
    remoteCalls.push(options)
    return { sentTo: null, starting: false }
  },
}))
vi.mock(`@/hooks/use-launch-composer`, () => ({
  useLaunchComposer: (args: { teamId: string; seed: unknown; onLanded?: () => void }) => {
    composerCalls.push({ teamId: args.teamId, seed: args.seed })
    composerState.onLanded = args.onLanded
    return {
      starting: composerState.starting,
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
  remoteCalls.length = 0
  composerState.starting = null
  composerState.onLanded = undefined
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

  it(`shuts once the started run lands, not at send time`, () => {
    const { rerender } = render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    // Starting: the watch that opens the run lives in the dialog, so it stays.
    composerState.starting = { deviceLabel: `buildbox`, since: 0 }
    rerender(<LaunchDialogHost team={team} />)
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    // The run's row landed: done here.
    act(() => {
      composerState.starting = null
      composerState.onLanded?.()
    })
    expect(screen.queryByTestId(`launch-dialog`)).toBeNull()
  })

  it(`refuses a dismissal while a start is pending, and stays after a timeout`, () => {
    const { rerender } = render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [`i1`] }))
    act(() => {
      composerState.starting = { deviceLabel: `buildbox`, since: 0 }
      rerender(<LaunchDialogHost team={team} />)
    })
    // Escape at the layer, and the close paths that funnel through
    // `onOpenChange(false)` (the ✕): both no-ops while the run is pending.
    fireEvent.keyDown(document.activeElement ?? document.body, { key: `Escape` })
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    fireEvent.click(screen.getByRole(`button`, { name: `Close` }))
    expect(screen.getByTestId(`launch-dialog`)).toBeTruthy()
    // A timeout (no landing) returns the composer to idle: the dialog stays,
    // and dismisses freely again.
    act(() => {
      composerState.starting = null
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

  it(`hands the request's origin to the run watch, and only when it was named`, () => {
    render(<LaunchDialogHost team={team} />)
    act(() => requestLaunchDialog({ issueIds: [], actionId: `a1`, origin: null }))
    expect(remoteCalls[0]).toMatchObject({ teamId: `t1`, origin: null })
    // The composer's seed never carries the origin: it is the watch's alone.
    expect(composerCalls[0]?.seed).toEqual({ issueIds: [], actionId: `a1` })
    act(() => requestLaunchDialog({ issueIds: [`i2`] }))
    const derived = remoteCalls[remoteCalls.length - 1]!
    expect(`origin` in derived).toBe(false)
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
