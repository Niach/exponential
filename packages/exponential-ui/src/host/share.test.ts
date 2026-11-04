// VAPP-241: fixtures/package-share.json replayed (the Rust core replays the
// same file in tests/package_share.rs).
import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/package-share.json" with { type: "json" }
import { validatePackageShares } from "./share"

describe(`package share (fixtures/package-share.json)`, () => {
  for (const c of fixture.cases) test(c.name, () => expect(validatePackageShares({ ...fixture.package, share: c.share })).toEqual(c.issues))
})
