import { describe, expect, it } from "vitest"
import viewFixture from "@exp/domain-contract/fixtures/ci-page-view.json"
import { replayCiPage } from "./ci-page-view-replay"

// #EXP-2172: the web reference replays every tree of ci-page-view.json (the
// desktop, iOS and Android builders replay the same file).
describe(`ci-page-view fixture`, () => {
  it.each(viewFixture.cases)(`$name`, (c) => {
    expect(JSON.parse(JSON.stringify(replayCiPage(c as never)))).toEqual(c.tree)
  })
})
