import { describe, expect, it } from "vitest"
import {
  guideSearch,
  guideSectionPage,
  parseGuideSearch,
  parseGuideSection,
  availableFaces,
  changesFaceCounts,
  codingTarget,
  GUIDE_FACE_LABEL,
  OPEN_RESULTS_LABEL,
  faceLabel,
  faceShowsContextMenu,
  fallbackFace,
  phaseDotTone,
  primaryAction,
  sessionModel,
  swipeTarget,
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
  it(`lists the available faces in issue, run, guide order`, () => {
    expect(
      availableFaces({ hasIssue: true, hasRun: true, hasResults: true, hasDiff: true })
    ).toEqual([`issue`, `run`, `guide`])
    expect(
      availableFaces({ hasIssue: false, hasRun: true, hasResults: false, hasDiff: false })
    ).toEqual([`run`])
    // EXP-1251: a diff alone (an open PR, no run of mine) is a Guide.
    expect(
      availableFaces({ hasIssue: true, hasRun: false, hasResults: false, hasDiff: true })
    ).toEqual([`issue`, `guide`])
    // So are results alone.
    expect(
      availableFaces({ hasIssue: true, hasRun: true, hasResults: true, hasDiff: false })
    ).toEqual([`issue`, `run`, `guide`])
  })

  it(`labels the faces, Runs once there are several`, () => {
    expect(faceLabel(`issue`)).toBe(`Issue`)
    expect(faceLabel(`run`)).toBe(`Run`)
    expect(faceLabel(`run`, true)).toBe(`Runs`)
    expect(faceLabel(`guide`)).toBe(`Guide`)
    expect(GUIDE_FACE_LABEL).toBe(`Guide`)
    expect(OPEN_RESULTS_LABEL).toBe(`Open Guide`)
    expect(STEER_COMPOSER_PLACEHOLDER).toBe(`Type / for commands`)
    expect(PLAN_MODE_LABEL).toBe(`Plan mode`)
  })

  it(`counts the diff once the files are known`, () => {
    expect(changesFaceCounts(null)).toBeNull()
    expect(changesFaceCounts(undefined)).toBeNull()
    expect(changesFaceCounts({ files: 0, additions: 0, deletions: 0 })).toBeNull()
    expect(changesFaceCounts({ files: 3, additions: 12, deletions: 2 })).toEqual({
      additions: 12,
      deletions: 2,
    })
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

  // EXP-1150: the faces are tabs — a swipe walks the strip.
  it(`swipes to the neighbouring face`, () => {
    const faces = [`issue`, `run`, `changes`, `results`] as const
    expect(swipeTarget(faces, `issue`, `left`)).toBe(`run`)
    expect(swipeTarget(faces, `run`, `left`)).toBe(`changes`)
    expect(swipeTarget(faces, `changes`, `right`)).toBe(`run`)
    expect(swipeTarget(faces, `run`, `right`)).toBe(`issue`)
    // Nothing past either end.
    expect(swipeTarget(faces, `issue`, `right`)).toBeNull()
    expect(swipeTarget(faces, `results`, `left`)).toBeNull()
    // A gap in the strip is skipped: the neighbour is the next AVAILABLE face.
    expect(swipeTarget([`issue`, `results`], `issue`, `left`)).toBe(`results`)
    // A face that is not in the strip swipes nowhere.
    expect(swipeTarget([`issue`], `run`, `left`)).toBeNull()
    expect(swipeTarget([], `issue`, `left`)).toBeNull()
    // EXP-1190: any phone tab strip walks the same way.
    const tabs = [`inbox`, `my-issues`, `drafts`]
    expect(swipeTarget(tabs, `inbox`, `left`)).toBe(`my-issues`)
    expect(swipeTarget(tabs, `drafts`, `left`)).toBeNull()
  })

  it(`falls back guide to run to issue`, () => {
    expect(fallbackFace(`guide`, [`issue`, `run`])).toBe(`run`)
    expect(fallbackFace(`guide`, [`issue`])).toBe(`issue`)
    expect(fallbackFace(`guide`, [`issue`, `run`, `guide`])).toBe(`guide`)
    expect(fallbackFace(`guide`, [])).toBeNull()
    expect(fallbackFace(`run`, [`issue`])).toBe(`issue`)
    expect(fallbackFace(`run`, [`issue`, `run`])).toBe(`run`)
    expect(fallbackFace(`issue`, [`run`])).toBe(`run`)
    expect(fallbackFace(`run`, [])).toBeNull()
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

  // EXP-934 — mirrored by iOS `WorkFacesTests` and Android `WorkFacesTest`.
  it(`shows the context menu on the issue face alone`, () => {
    expect(faceShowsContextMenu(`issue`)).toBe(true)
    expect(faceShowsContextMenu(`run`)).toBe(false)
    expect(faceShowsContextMenu(`guide`)).toBe(false)
  })
})

// EXP-933: the issue's Results run, fixture-locked ×4.
import resultsFixture from "@exp/domain-contract/fixtures/session-results.json"
import { issueResultsRun } from "./work-faces"

describe(`issueResultsRun`, () => {
  const now = new Date(resultsFixture.issueResultsRun.now)
  for (const c of resultsFixture.issueResultsRun.cases) {
    it(c.name, () => {
      const rows = c.rows as Array<{
        id: string
        issueId: string
        userId: string
        status: string
        startedAt: string
        updatedAt: string
        results: unknown
      }>
      const prUrl = (c as { prUrl?: string | null }).prUrl ?? null
      const picked = issueResultsRun(rows, c.issueId, c.boundId, c.me ?? undefined, now, prUrl)
      expect(picked?.id ?? null).toBe(c.expected)
    })
  }
})

// EXP-1175: the Run face status row, fixture-locked ×4.
import runRowFixture from "@exp/domain-contract/fixtures/run-row.json"
import {
  HIDE_WORK_LABEL,
  SHOW_WORK_DEFAULT,
  SHOW_WORK_LABEL,
  runRowCaption,
  runRowState,
  showWorkLabel,
  turnRowCaption,
  type RunRowState,
} from "./work-faces"

describe(`run row`, () => {
  it(`labels the Show work switch off the fixture`, () => {
    expect(SHOW_WORK_LABEL).toBe(runRowFixture.showWorkLabel)
    expect(HIDE_WORK_LABEL).toBe(runRowFixture.hideWorkLabel)
    expect(SHOW_WORK_DEFAULT).toBe(runRowFixture.showWorkDefault)
    expect(showWorkLabel(false)).toBe(runRowFixture.showWorkLabel)
    expect(showWorkLabel(true)).toBe(runRowFixture.hideWorkLabel)
  })

  for (const c of runRowFixture.captions) {
    it(c.name, () => {
      expect(
        runRowCaption({
          state: c.state as RunRowState,
          device: c.device,
          startedAt: c.startedAt,
          endedAt: c.endedAt,
          now: c.now,
        })
      ).toEqual(c.expected)
    })
  }
})

describe(`runRowState`, () => {
  for (const c of runRowFixture.states) {
    it(c.name, () => {
      expect(
        runRowState({
          paused: c.paused,
          ended: c.ended,
          awaitingInput: c.awaitingInput,
          working: c.working,
          display: c.display as `needs_input` | `working` | `review` | `done`,
        })
      ).toBe(c.expected)
    })
  }
})

// EXP-1245: one status row per turn, fixture-locked ×4.
describe(`turnRowCaption`, () => {
  for (const c of runRowFixture.turnCaptions) {
    it(c.name, () => {
      expect(
        turnRowCaption({
          turn: c.turn,
          state: c.state as RunRowState,
          device: c.device,
          runEndedAt: c.runEndedAt,
          now: c.now,
        })
      ).toEqual(c.expected)
    })
  }
})

// EXP-1251: the Guide's URL (`?view=guide&section=&file=`, the legacy views
// normalised into it) and the section page a Changes row opens.
describe(`the Guide URL`, () => {
  it(`parses a section: a 1-based number, lead, other, all; garbage = none`, () => {
    expect(parseGuideSection(`3`)).toBe(3)
    expect(parseGuideSection(2)).toBe(2)
    expect(parseGuideSection(`lead`)).toBe(`lead`)
    expect(parseGuideSection(`other`)).toBe(`other`)
    expect(parseGuideSection(`all`)).toBe(`all`)
    expect(parseGuideSection(`0`)).toBeUndefined()
    expect(parseGuideSection(`1.5`)).toBeUndefined()
    expect(parseGuideSection(`x`)).toBeUndefined()
    expect(parseGuideSection(undefined)).toBeUndefined()
  })

  it(`normalises the legacy views into the Guide`, () => {
    expect(parseGuideSearch({ view: `results` })).toEqual({ view: `guide` })
    expect(parseGuideSearch({ view: `diff`, file: `a.ts` })).toEqual({
      view: `guide`,
      section: `all`,
      file: `a.ts`,
    })
    expect(parseGuideSearch({ view: `guide`, section: `2`, file: `a.ts` })).toEqual({
      view: `guide`,
      section: 2,
      file: `a.ts`,
    })
  })

  it(`drops a file without a section and every key without the Guide`, () => {
    expect(parseGuideSearch({ view: `guide`, file: `a.ts` })).toEqual({ view: `guide` })
    expect(parseGuideSearch({ section: `2`, file: `a.ts` })).toEqual({})
    expect(parseGuideSearch({ view: `changes` })).toEqual({})
  })

  it(`writes the Guide's search`, () => {
    expect(guideSearch({ from: `inbox` })).toEqual({ from: `inbox`, view: `guide` })
    expect(guideSearch({ section: `other`, file: `b.ts` })).toEqual({
      view: `guide`,
      section: `other`,
      file: `b.ts`,
    })
    expect(guideSearch({ file: `b.ts` })).toEqual({ view: `guide` })
  })
})

describe(`guideSectionPage`, () => {
  const file = (path: string, additions: number, deletions: number, previousPath?: string) => ({
    path,
    previousPath,
    additions,
    deletions,
  })
  const files = [file(`a.ts`, 3, 1), file(`b.ts`, 5, 0, `old-b.ts`), file(`c.ts`, 1, 1)]
  const groups = [
    { topic: `Summary`, files: [] },
    { topic: `Model`, files: [`a.ts`] },
    { topic: `Paint`, files: [`old-b.ts`] },
  ]

  it(`opens a numbered section with its caption and covered files`, () => {
    expect(guideSectionPage(groups, files, 2)).toEqual({
      section: 2,
      caption: `02 / 02`,
      title: `Paint`,
      files: [files[1]],
      additions: 5,
      deletions: 0,
    })
  })

  it(`opens Other changes and the complete diff`, () => {
    expect(guideSectionPage(groups, files, `other`)?.files).toEqual([files[2]])
    expect(guideSectionPage(groups, files, `other`)?.title).toBe(`Other changes`)
    const all = guideSectionPage(groups, files, `all`)
    expect(all?.files).toHaveLength(3)
    expect(all?.caption).toBeNull()
    expect([all?.additions, all?.deletions]).toEqual([9, 2])
  })

  it(`is null for a stale section or while the diff loads`, () => {
    expect(guideSectionPage(groups, files, 7)).toBeNull()
    expect(guideSectionPage(groups, null, 1)).toBeNull()
    expect(guideSectionPage([{ topic: `Model`, files: [`a.ts`, `b.ts`, `c.ts`] }], files, `other`)).toBeNull()
  })

  it(`with no report the whole diff is one Changes section`, () => {
    expect(guideSectionPage([], files, `other`)?.title).toBe(`Changes`)
  })
})
