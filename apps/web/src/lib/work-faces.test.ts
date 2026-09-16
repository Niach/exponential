import { describe, expect, it } from "vitest"
import {
  availableFaces,
  codingTarget,
  faceLabel,
  fallbackFace,
  phaseDotTone,
  primaryAction,
  sessionModel,
  switcherBadge,
  switcherMode,
  switcherTargets,
  PLAN_MODE_LABEL,
  STEER_COMPOSER_PLACEHOLDER,
} from "./work-faces"

// EXP-893: the phone Work screen's pure rules. Test names are mirrored by
// iOS `WorkFacesTests.swift` and Android `WorkFacesTest.kt`.

const now = new Date(`2026-09-15T12:00:00Z`)

function run(
  id: string,
  over: Partial<{
    issueId: string | null
    userId: string
    status: string
    startedAt: string
    updatedAt: string
  }> = {}
) {
  return {
    id,
    issueId: `issue-1`,
    userId: `me`,
    status: `running`,
    startedAt: `2026-09-15T11:00:00Z`,
    updatedAt: `2026-09-15T11:59:00Z`,
    ...over,
  }
}

describe(`work faces`, () => {
  it(`lists the available faces in issue, run, changes, results order`, () => {
    expect(
      availableFaces({
        hasIssue: true,
        hasRun: true,
        hasChanges: true,
        hasResults: true,
      })
    ).toEqual([`issue`, `run`, `changes`, `results`])
    expect(
      availableFaces({
        hasIssue: false,
        hasRun: true,
        hasChanges: false,
        hasResults: false,
      })
    ).toEqual([`run`])
    // Changes is independent of Run: an open PR with no run of mine.
    expect(
      availableFaces({
        hasIssue: true,
        hasRun: false,
        hasChanges: true,
        hasResults: false,
      })
    ).toEqual([`issue`, `changes`])
    // EXP-879: results come LAST, after the changes face.
    expect(
      availableFaces({
        hasIssue: true,
        hasRun: true,
        hasChanges: false,
        hasResults: true,
      })
    ).toEqual([`issue`, `run`, `results`])
  })

  it(`labels the faces, Runs once there are several`, () => {
    expect(faceLabel(`issue`)).toBe(`Issue`)
    expect(faceLabel(`run`)).toBe(`Run`)
    expect(faceLabel(`run`, true)).toBe(`Runs`)
    expect(faceLabel(`changes`)).toBe(`Changes`)
    expect(faceLabel(`results`)).toBe(`Results`)
    expect(STEER_COMPOSER_PLACEHOLDER).toBe(`Type / for commands`)
    expect(PLAN_MODE_LABEL).toBe(`Plan mode`)
  })

  it(`targets the bound run when it is mine and live`, () => {
    const rows = [
      run(`a`, { startedAt: `2026-09-15T10:00:00Z` }),
      run(`b`, { startedAt: `2026-09-15T11:00:00Z` }),
    ]
    expect(codingTarget(rows, `issue-1`, `a`, `me`, now)?.id).toBe(`a`)
  })

  it(`targets the newest live own run, else the newest own run`, () => {
    const rows = [
      run(`old-live`, { startedAt: `2026-09-15T09:00:00Z` }),
      run(`new-live`, { startedAt: `2026-09-15T11:00:00Z` }),
      run(`newest-ended`, {
        status: `ended`,
        startedAt: `2026-09-15T11:30:00Z`,
      }),
      run(`theirs`, { userId: `them`, startedAt: `2026-09-15T11:45:00Z` }),
    ]
    expect(codingTarget(rows, `issue-1`, undefined, `me`, now)?.id).toBe(
      `new-live`
    )
    // A bound run that ENDED does not win over a live one.
    expect(codingTarget(rows, `issue-1`, `newest-ended`, `me`, now)?.id).toBe(
      `new-live`
    )
    const ended = rows.filter((row) => row.status === `ended`)
    expect(codingTarget(ended, `issue-1`, undefined, `me`, now)?.id).toBe(
      `newest-ended`
    )
    expect(codingTarget(rows, `issue-2`, undefined, `me`, now)).toBeNull()
    expect(codingTarget(rows, `issue-1`, undefined, undefined, now)).toBeNull()
  })

  it(`treats a stale running row as not live`, () => {
    const rows = [
      run(`stale`, {
        startedAt: `2026-09-15T11:00:00Z`,
        updatedAt: `2026-09-15T08:00:00Z`,
      }),
      run(`ended`, { status: `ended`, startedAt: `2026-09-15T10:00:00Z` }),
    ]
    // No live run: the newest own run is the stale one — still the target.
    expect(codingTarget(rows, `issue-1`, undefined, `me`, now)?.id).toBe(
      `stale`
    )
    expect(codingTarget(rows, `issue-1`, `stale`, `me`, now)?.id).toBe(`stale`)
  })

  it(`picks the primary action, stop first`, () => {
    expect(
      primaryAction({ ownLive: true, ownEndedResumable: true, canStart: true })
    ).toBe(`stop`)
    expect(
      primaryAction({ ownLive: false, ownEndedResumable: true, canStart: true })
    ).toBe(`resume`)
    expect(
      primaryAction({ ownLive: false, ownEndedResumable: false, canStart: true })
    ).toBe(`start`)
    expect(
      primaryAction({ ownLive: false, ownEndedResumable: false, canStart: false })
    ).toBe(`none`)
  })

  it(`offers the other faces as switcher targets`, () => {
    expect(
      switcherTargets([`issue`, `run`, `changes`], `run`, [`a`], `a`, false)
    ).toEqual([
      { kind: `face`, face: `issue` },
      { kind: `face`, face: `changes` },
    ])
    expect(switcherTargets([`issue`], `issue`, [], null, false)).toEqual([])
  })

  it(`expands the run face into one row per run with two or more`, () => {
    expect(
      switcherTargets([`issue`, `run`], `issue`, [`a`, `b`], `a`, false)
    ).toEqual([
      { kind: `run`, id: `a` },
      { kind: `run`, id: `b` },
    ])
    // On the Run face the shown run is not a target.
    expect(
      switcherTargets([`issue`, `run`, `changes`], `run`, [`a`, `b`], `a`, false)
    ).toEqual([
      { kind: `face`, face: `issue` },
      { kind: `run`, id: `b` },
      { kind: `face`, face: `changes` },
    ])
  })

  it(`prepends start coding when the shown run ended for good`, () => {
    expect(switcherTargets([`issue`, `run`], `run`, [`a`], `a`, true)).toEqual([
      { kind: `startCoding` },
      { kind: `face`, face: `issue` },
    ])
  })

  it(`hides, toggles or opens a menu by target count`, () => {
    expect(switcherMode([])).toEqual({ kind: `hidden` })
    expect(switcherMode([{ kind: `face`, face: `run` }])).toEqual({
      kind: `toggle`,
      target: { kind: `face`, face: `run` },
    })
    expect(
      switcherMode([
        { kind: `face`, face: `issue` },
        { kind: `face`, face: `changes` },
      ]).kind
    ).toBe(`menu`)
  })

  it(`badges the circle with the session tone off the run face`, () => {
    expect(switcherBadge(`issue`, `running`, true)).toBe(`running`)
    expect(switcherBadge(`changes`, `needs_input`, false)).toBe(`needs_input`)
    expect(switcherBadge(`issue`, null, true)).toBeNull()
  })

  it(`badges the circle with changes on the run face`, () => {
    expect(switcherBadge(`run`, `running`, true)).toBe(`changes`)
    expect(switcherBadge(`run`, `running`, false)).toBeNull()
  })

  it(`falls back changes to run to issue`, () => {
    expect(fallbackFace(`changes`, [`issue`, `run`])).toBe(`run`)
    expect(fallbackFace(`changes`, [`issue`])).toBe(`issue`)
    expect(fallbackFace(`run`, [`issue`])).toBe(`issue`)
    expect(fallbackFace(`run`, [`issue`, `run`])).toBe(`run`)
    expect(fallbackFace(`issue`, [`run`])).toBe(`run`)
    expect(fallbackFace(`run`, [])).toBeNull()
    // EXP-879: the results face walks the same ladder.
    expect(fallbackFace(`results`, [`issue`, `run`])).toBe(`run`)
    expect(fallbackFace(`results`, [`issue`])).toBe(`issue`)
    expect(fallbackFace(`results`, [`issue`, `run`, `results`])).toBe(`results`)
    expect(fallbackFace(`results`, [])).toBeNull()
  })

  it(`reads the session model off the config option`, () => {
    expect(sessionModel(null)).toBeNull()
    expect(sessionModel({ options: [] })).toBeNull()
    expect(sessionModel({ options: [{ id: `model`, value: `` }] })).toBeNull()
    expect(sessionModel({ options: [{ id: `model`, value: `opus` }] })).toBe(
      `opus`
    )
  })

  it(`tones the state dot off the phase`, () => {
    const base = {
      live: true,
      connecting: false,
      awaitingInput: false,
      paused: false,
      stale: false,
    }
    expect(phaseDotTone(base)).toEqual({ tone: `running`, connecting: false })
    expect(phaseDotTone({ ...base, awaitingInput: true }).tone).toBe(
      `needs_input`
    )
    expect(phaseDotTone({ ...base, stale: true }).tone).toBe(`needs_input`)
    expect(phaseDotTone({ ...base, paused: true }).tone).toBe(`muted`)
    expect(phaseDotTone({ ...base, live: false }).tone).toBe(`muted`)
    expect(
      phaseDotTone({ ...base, live: false, connecting: true }).connecting
    ).toBe(true)
    expect(
      phaseDotTone({ ...base, live: false, connecting: true, paused: true })
        .connecting
    ).toBe(false)
  })
})
