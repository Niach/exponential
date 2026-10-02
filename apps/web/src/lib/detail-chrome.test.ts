import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/detail-chrome.json"
import { DETAIL_CHROME, isTitleCollapsed } from "./detail-chrome"
import { faceDots, type WorkFaceKind } from "./work-faces"

// EXP-1162: the detail chrome rule, locked ×4 (Android DetailChromeTest, iOS
// DetailChromeTests, desktop domain::detail_chrome) against the ONE contract
// fixture — same cases, same test names.
describe(`detail chrome (contract fixture)`, () => {
  for (const c of fixture.collapse) {
    it(c.name, () => {
      expect(
        isTitleCollapsed(c.hasTitleRow, c.titleBottom, c.headerBottom)
      ).toBe(c.collapsed)
    })
  }

  it(`carries the contract constants`, () => {
    expect(DETAIL_CHROME).toEqual({
      collapseMs: 160,
      collapseRise: 4,
      edgeTop: 24,
      edgeBottom: 32,
      edgeBlur: 8,
      scrim: 0.72,
      faceDot: 6,
      faceDotGap: 6,
      faceMark: 14,
      faceMarkGap: 6,
      faceMarkBadge: 6,
    })
  })
})

describe(`face dots (contract fixture)`, () => {
  for (const c of fixture.faceDots) {
    it(c.name, () => {
      expect(
        faceDots({
          faces: c.faces as WorkFaceKind[],
          runLive: c.runLive,
          needsInput: c.needsInput,
          prOpen: c.prOpen,
        })
      ).toEqual(c.dots)
    })
  }
})
