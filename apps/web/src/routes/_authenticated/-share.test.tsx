// EXP-1635: the Share to page against share-targets.json: the rows and their
// order, the preview line, the team selector, and what each pick navigates
// to. Nothing here sends. Lives under a `-` prefix so the route generator
// ignores it.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/share-targets.json"
import { TEMPORARY_PICK } from "@exp/domain-contract/composer-agent-row"
import type { Team } from "@/db/schema"
import type { PendingShare } from "@/lib/share/pending-share"

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  teams: [] as unknown[],
  agents: [] as unknown[],
  ensured: [] as string[],
}))

vi.mock(`@tanstack/react-router`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useNavigate: () => mocks.navigate,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: () => ({ data: mocks.agents }),
}))
vi.mock(`@/lib/collections`, () => ({ agentCollection: {} }))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamMemberships: () => ({ myTeams: mocks.teams }),
}))
vi.mock(`@/hooks/use-ensure-assistant`, () => ({
  useEnsureAssistant: (teamId: string) => mocks.ensured.push(teamId),
}))
vi.mock(`@/lib/last-visited`, () => ({ readLastVisited: () => ({ teamSlug: `acme` }) }))

const { ShareTargetsPage, shareTeam } = await import(`@/components/share-targets-page`)

const team = (slug: string): Team => ({ id: `id-${slug}`, slug, name: slug, iconUrl: null }) as Team
const share = (over: Partial<PendingShare> = {}): PendingShare => ({
  title: ``,
  text: `hello`,
  url: ``,
  files: [],
  ...over,
})
const png = (name: string) => new File([`x`], name, { type: `image/png` })
const rowIds = () =>
  screen.getAllByTestId(/^share-target-/).map((row) => row.getAttribute(`data-testid`)!)
const draw = (pending = share()) =>
  render(<ShareTargetsPage share={pending} defaultTeamSlug="acme" me="u1" />)

describe(`ShareTargetsPage`, () => {
  beforeEach(() => {
    mocks.navigate.mockClear()
    mocks.teams = [team(`acme`)]
    mocks.agents = []
    mocks.ensured = []
    URL.createObjectURL = vi.fn(() => `blob:x`)
    URL.revokeObjectURL = vi.fn()
  })

  for (const testCase of fixture.targetCases) {
    it(testCase.name, () => {
      mocks.agents = testCase.input.agents
      render(<ShareTargetsPage share={share()} defaultTeamSlug="acme" me={testCase.input.me} />)
      expect(rowIds()).toEqual(
        [...testCase.expected.rows, ...testCase.expected.agents].map((id) => `share-target-${id}`)
      )
      if (testCase.expected.agents.length === 0) expect(screen.queryByText(fixture.copy.agents)).toBeNull()
      else expect(screen.getByText(fixture.copy.agents)).toBeTruthy()
    })
  }

  it(`the fixture's copy on the page`, () => {
    draw()
    expect(screen.getByTestId(fixture.testIds.page).textContent).toContain(fixture.copy.title)
    expect(screen.getByText(fixture.copy.newIssue)).toBeTruthy()
    expect(screen.getByText(fixture.copy.newChat)).toBeTruthy()
    expect(screen.getByText(fixture.copy.newChatSubtitle)).toBeTruthy()
  })

  it(`the viewer's Assistant is ensured on the page's team`, () => {
    draw()
    expect(mocks.ensured).toContain(`id-acme`)
  })

  describe(`the preview line`, () => {
    it(`none for a text share`, () => {
      draw()
      expect(screen.queryByTestId(fixture.testIds.preview)).toBeNull()
    })

    it(`none for a non-image file`, () => {
      draw(share({ files: [new File([`x`], `a.pdf`, { type: `application/pdf` })] }))
      expect(screen.queryByTestId(fixture.testIds.preview)).toBeNull()
    })

    it(`one image`, () => {
      draw(share({ files: [png(`a.png`)] }))
      expect(screen.getByTestId(fixture.testIds.preview).textContent).toContain(fixture.copy.imagesOne)
    })

    it(`many images`, () => {
      draw(share({ files: [png(`a.png`), png(`b.png`), png(`c.png`)] }))
      expect(screen.getByTestId(fixture.testIds.preview).textContent).toContain(`3 images`)
    })
  })

  describe(`the picks only navigate`, () => {
    it(`New issue opens a draft on the share`, () => {
      draw()
      fireEvent.click(screen.getByTestId(`share-target-new-issue`))
      expect(mocks.navigate).toHaveBeenCalledExactlyOnceWith(
        expect.objectContaining({
          to: `/t/$teamSlug/drafts/$draftId`,
          params: expect.objectContaining({ teamSlug: `acme` }),
          search: { share: `1` },
        })
      )
    })

    it(`New chat opens the composer on Temporary session`, () => {
      draw()
      fireEvent.click(screen.getByTestId(`share-target-new-chat`))
      expect(mocks.navigate).toHaveBeenCalledExactlyOnceWith({
        to: `/t/$teamSlug/agent`,
        params: { teamSlug: `acme` },
        search: { share: TEMPORARY_PICK },
      })
    })

    it(`an agent opens the composer on that agent`, () => {
      mocks.agents = [{ id: `a1`, name: `Triage`, sortOrder: 1, role: `agent`, ownerId: `u2` }]
      draw()
      fireEvent.click(screen.getByTestId(`share-target-a1`))
      expect(mocks.navigate).toHaveBeenCalledExactlyOnceWith({
        to: `/t/$teamSlug/agent`,
        params: { teamSlug: `acme` },
        search: { share: `a1` },
      })
    })
  })

  describe(`the team`, () => {
    it(`one team: no selector`, () => {
      draw()
      expect(screen.queryByTestId(`share-targets-team`)).toBeNull()
    })

    it(`more than one: the selector`, () => {
      mocks.teams = [team(`other`), team(`acme`)]
      draw()
      expect(screen.getByTestId(`share-targets-team`).textContent).toContain(`acme`)
    })

    it(`last visited wins, then the default, then the first`, () => {
      const teams = [team(`a`), team(`b`), team(`c`)]
      expect(shareTeam(teams, `c`, `b`)?.slug).toBe(`c`)
      expect(shareTeam(teams, `gone`, `b`)?.slug).toBe(`b`)
      expect(shareTeam(teams, undefined, `gone`)?.slug).toBe(`a`)
    })
  })
})
