import { describe, expect, it } from "vitest"
import { recentPanelSurvives, RECENT_RUNS_ORIGIN } from "@/lib/recent-runs-panel"

// EXP-1246: the Recent panel is a list-detail list — it survives the runs it
// opens and drops everywhere else.
describe(`recentPanelSurvives`, () => {
  it(`leaves the Agent page's own toggle alone`, () => {
    expect(recentPanelSurvives(`/t/acme/agent`, null)).toBe(`keep`)
    expect(recentPanelSurvives(`/t/acme/agent/`, `inbox`)).toBe(`keep`)
  })

  it(`opens on a detail opened from Recent`, () => {
    expect(RECENT_RUNS_ORIGIN).toBe(`agent:recent`)
    expect(recentPanelSurvives(`/t/acme/sessions/s1`, `agent:recent`)).toBe(`open`)
    expect(
      recentPanelSurvives(`/t/acme/boards/web/issues/MET-1`, `agent:recent`)
    ).toBe(`open`)
  })

  it(`closes everywhere else`, () => {
    for (const [path, from] of [
      [`/t/acme/sessions/s1`, `agent`],
      [`/t/acme/sessions/s1`, null],
      [`/t/acme/boards/web/issues/MET-1`, `inbox`],
      [`/t/acme/inbox`, null],
      [`/t/acme/boards/web`, `agent:recent`],
      [`/onboarding`, null],
    ] as const) {
      expect(recentPanelSurvives(path, from), `${path} ${from}`).toBe(`close`)
    }
  })
})
