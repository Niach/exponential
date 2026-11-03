// #EXP-1632 (settings-nav.json `apps`): web-mobile's Team page draws ONE Apps
// row after Skills and before Devices, opening Settings › Apps, and never
// the nav's injected `@apps` / `@new-app` lists. Lives under a `-` prefix so
// the route generator ignores it.
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
  it.each([true, false])(`draws exactly one Apps row between Skills and Devices (owner %s)`, (isOwner) => {
    const ids = idsFor(isOwner)
    expect(ids.filter((id) => id.includes(`app`))).toEqual([`apps`])
    expect(ids.indexOf(`apps`)).toBe(ids.indexOf(`skills`) + 1)
    expect(ids.indexOf(`devices`)).toBe(ids.indexOf(`apps`) + 1)
    expect(ids.some((id) => id.includes(`@`))).toBe(false)
  })

  it(`opens Settings › Apps`, () => {
    expect(teamRowPath(`apps`)).toBe(`/t/$teamSlug/settings/apps`)
    expect(teamRowPath(`devices`)).toBe(`/t/$teamSlug/devices`)
  })
})
