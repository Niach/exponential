// VAPP-242 (settings-nav.json, replacing #EXP-1632): web-mobile's Team page
// draws NO Apps row (the Apps tab lists the apps) and never the nav's
// injected `@apps` / `@new-app` lists; Devices follows Skills. Lives under a
// `-` prefix so the route generator ignores it.
import { describe, expect, it, vi } from "vitest"
import { settingsNavVisible } from "@exp/domain-contract/settings-nav"

vi.mock(`@tanstack/react-router`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  createFileRoute: () => (options: Record<string, unknown>) => options,
}))

const { teamPageRows, teamRowPath } = await import(`./team`)

const idsFor = (isOwner: boolean) =>
  teamPageRows(
    (row) => settingsNavVisible(row, { isOwner, isCloud: false }),
    () => true
  ).map((entry) => (entry.kind === `row` ? entry.row.id : `page:${entry.item.id}`))

describe(`Team page rows (settings-nav.json)`, () => {
  it.each([true, false])(`draws no Apps row; Devices follows Skills (owner %s)`, (isOwner) => {
    const ids = idsFor(isOwner)
    expect(ids.filter((id) => id.includes(`app`))).toEqual([])
    expect(ids.indexOf(`devices`)).toBe(ids.indexOf(`skills`) + 1)
    expect(ids.some((id) => id.includes(`@`))).toBe(false)
  })

  it(`opens Devices`, () => {
    expect(teamRowPath(`devices`)).toBe(`/t/$teamSlug/devices`)
  })
})
