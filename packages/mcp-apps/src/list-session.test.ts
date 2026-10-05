import {
  ago,
  blockedBadgeLabel,
  pastRunByline,
  sessionAgentCaption,
  sessionBands,
  sessionDisplayState,
  sessionIdentity,
  sessionRowIsWorking,
  sessionStatusLine,
  type SessionListRow,
} from "./list-session"

const NOW = Date.parse(`2026-10-05T12:00:00Z`)
const run = (id: string, extra: Partial<SessionListRow> = {}): SessionListRow => ({
  id,
  status: `running`,
  createdAt: `2026-10-05T10:00:00Z`,
  ...extra,
})

describe(`sessionDisplayState`, () => {
  it(`follows the session-display fixture order`, () => {
    expect(sessionDisplayState(run(`a`, { needsInput: true, agentBusy: true }))).toBe(`needs_input`)
    expect(sessionDisplayState(run(`a`, { agentBusy: true, status: `in_review` }))).toBe(`working`)
    expect(sessionDisplayState(run(`a`, { status: `in_review`, prState: `open` }))).toBe(`review`)
    expect(sessionDisplayState(run(`a`, { status: `in_review`, prState: `merged` }))).toBe(`done`)
    expect(sessionDisplayState(run(`a`))).toBe(`done`)
  })

  it(`never animates an ended row`, () => {
    expect(sessionRowIsWorking(run(`a`, { agentBusy: true }))).toBe(true)
    expect(sessionRowIsWorking(run(`a`, { agentBusy: true, status: `ended` }))).toBe(false)
  })
})

describe(`row lines`, () => {
  it(`captions live rows only`, () => {
    expect(sessionAgentCaption(run(`a`, { agentCaption: ` Reading files ` }))).toBe(`Reading files`)
    expect(sessionAgentCaption(run(`a`, { agentCaption: `x`, status: `ended` }))).toBeNull()
  })

  it(`writes the status line per state`, () => {
    expect(sessionStatusLine(run(`a`, { needsInput: true, deviceLabel: `mac` }), NOW)).toEqual({
      text: `Needs input · mac`,
      tone: `amber`,
    })
    expect(
      sessionStatusLine(run(`a`, { agentBusy: true, startedAt: `2026-10-05T11:55:00Z` }), NOW)
    ).toEqual({ text: `Desktop · started 5m ago`, tone: `muted` })
    expect(sessionStatusLine(run(`a`, { status: `in_review`, prState: `open` }), NOW).tone).toBe(
      `emerald`
    )
  })

  it(`bylines an ended run with its device and end`, () => {
    expect(
      pastRunByline(run(`a`, { status: `ended`, deviceLabel: `box`, endedAt: `2026-10-05T09:00:00Z` }), NOW)
    ).toBe(`box · 3h ago`)
    expect(pastRunByline(run(`a`, { status: `ended` }), NOW)).toBe(``)
  })

  it(`labels the usage wall`, () => {
    expect(blockedBadgeLabel(null)).toBeNull()
    expect(blockedBadgeLabel({ resetsAt: `2026-10-05T14:10:00Z` }, NOW)).toBe(
      `Rate limited · resets in 2h 10m`
    )
    expect(blockedBadgeLabel({ kind: `other` }, NOW)).toBe(`Blocked`)
  })

  it(`names issue, action and batch runs`, () => {
    expect(sessionIdentity(run(`a`, { issueIdentifier: `EXP-1`, issueTitle: `Fix` }))).toEqual({
      identifier: `EXP-1`,
      subject: `Fix`,
    })
    expect(sessionIdentity(run(`a`, { actionName: `Chat` })).subject).toBe(`Chat`)
    expect(sessionIdentity(run(`a`)).subject).toBe(`Batch run`)
  })

  it(`says just now / N ago`, () => {
    expect(ago(`2026-10-05T11:59:50Z`, NOW)).toBe(`just now`)
    expect(ago(`2026-10-03T12:00:00Z`, NOW)).toBe(`2d ago`)
    expect(ago(null, NOW)).toBe(``)
  })
})

describe(`sessionBands`, () => {
  it(`bands by the root and nests children in creation order`, () => {
    const rows = [
      run(`late-child`, { parentSessionId: `root`, status: `ended`, createdAt: `2026-10-05T11:00:00Z` }),
      run(`root`),
      run(`early-child`, { parentSessionId: `root`, status: `in_review`, createdAt: `2026-10-05T10:30:00Z` }),
      run(`review`, { status: `in_review` }),
      run(`old`, { status: `ended` }),
    ]
    const bands = sessionBands(rows)
    expect(bands.map((band) => band.label)).toEqual([`Running`, `In review`, `Ended`])
    expect(bands[0]!.rows.map((entry) => [entry.row.id, entry.depth, entry.hasChildren])).toEqual([
      [`root`, 0, true],
      [`early-child`, 1, false],
      [`late-child`, 1, false],
    ])
    expect(bands[0]!.count).toBe(3)
  })

  it(`keeps an orphan child at the top and folds collapsed subtrees`, () => {
    const rows = [run(`root`), run(`kid`, { parentSessionId: `root` }), run(`orphan`, { parentSessionId: `gone` })]
    const [running] = sessionBands(rows, new Set([`root`]))
    expect(running!.rows.map((entry) => entry.row.id)).toEqual([`root`, `orphan`])
    expect(running!.count).toBe(3)
  })

  it(`collapses a resume into its successor and re-parents its children`, () => {
    const rows = [
      run(`new`, { resumedFromId: `old` }),
      run(`old`, { status: `ended` }),
      run(`kid`, { parentSessionId: `old` }),
    ]
    const bands = sessionBands(rows)
    expect(bands).toHaveLength(1)
    expect(bands[0]!.rows.map((entry) => [entry.row.id, entry.depth])).toEqual([
      [`new`, 0],
      [`kid`, 1],
    ])
  })

  it(`survives a parent cycle`, () => {
    const rows = [run(`a`, { parentSessionId: `b` }), run(`b`, { parentSessionId: `a` })]
    const ids = sessionBands(rows).flatMap((band) => band.rows.map((entry) => entry.row.id))
    expect(ids.sort()).toEqual([`a`, `b`])
  })
})
