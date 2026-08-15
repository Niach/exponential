// REV-32 — drift gates for the committed codegen outputs of
// @exp/domain-contract and @exp/design-tokens.
//
// Both generators write Swift, Kotlin, and Rust outputs in one run, but only
// the Rust files were CI-gated (build-desktop.yml's codegen-drift job, and
// only on desktop-v* tags). The mobile lock tests compare hand-written enums
// against the committed GENERATED file — so a stale generated file keeps both
// stale sides agreeing and everything green. This test is the sibling of
// icons.test.ts's "committed generated output is current": re-run each
// generator and byte-compare every output it owns.

import { describe, expect, it } from "vitest"
import { execFileSync } from "node:child_process"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)

const GENERATORS: { filter: string; pkg: string; outputs: string[] }[] = [
  {
    filter: `@exp/domain-contract`,
    pkg: `packages/domain-contract`,
    outputs: [
      `apps/ios/ExpCore/Sources/Domain/DomainContract.generated.swift`,
      `apps/android/app/src/main/java/com/exponential/app/domain/DomainContract.generated.kt`,
      `apps/desktop/crates/domain/src/contract.generated.rs`,
    ],
  },
  {
    filter: `@exp/design-tokens`,
    pkg: `packages/design-tokens`,
    outputs: [
      `apps/ios/ExpUI/Sources/DesignTokens.generated.swift`,
      `apps/android/app/src/main/java/com/exponential/app/ui/theme/DesignTokens.generated.kt`,
      `apps/desktop/crates/theme/src/tokens.generated.rs`,
    ],
  },
]

describe(`codegen drift`, () => {
  for (const { filter, pkg, outputs } of GENERATORS) {
    it(`${filter}: committed generated output is current`, () => {
      // Re-run the generator and assert nothing moved. Deliberately compares
      // file contents rather than `git status`, so it behaves the same on a
      // clean checkout, a dirty tree, and in CI.
      const targets = outputs.map((f) => join(repoRoot, f))
      const before = new Map(targets.map((f) => [f, readFileSync(f, `utf8`)]))
      execFileSync(`bun`, [`run`, `scripts/generate.ts`], {
        cwd: join(repoRoot, pkg),
        stdio: `pipe`,
      })
      for (const [file, contents] of before) {
        expect(
          readFileSync(file, `utf8`),
          `${file} is stale — run \`bun run --filter ${filter} generate\``
        ).toBe(contents)
      }
    })
  }
})
