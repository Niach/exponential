// EXP-380 — gates for the non-OSS corner of the dependency graph.
//
// Remotion is source-available, not open source. It is licensed to US on the
// basis of our headcount, it has no sublicence clause, and its npm tarball ships
// no LICENSE.md at all. docs/third-party-licences.md records what we are and are
// not permitted to do with it; this file stops those determinations from going
// quietly stale. Sibling of icons.test.ts, which locks the icon registry the
// same way — read repo files, byte-compare, fail loudly.

import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const read = (path: string) => readFileSync(join(repoRoot, path), `utf8`)

const dockerfile = read(`Dockerfile`)
const policy = read(`docs/third-party-licences.md`)
const remotionLicence = read(`docs/licences/remotion-LICENSE.txt`)
const marketingPkg = JSON.parse(read(`apps/marketing/package.json`)) as {
  dependencies: Record<string, string>
  devDependencies: Record<string, string>
}

/* The runtime stage is everything after the LAST `FROM` — that is the layer set
   that becomes ghcr.io/niach/exponential-web. The builder stage installs the
   whole workspace on purpose and is discarded. */
const runtimeStage = dockerfile.slice(dockerfile.lastIndexOf(`\nFROM `))

describe(`published image dependency scope`, () => {
  it(`scopes the runtime install to @exp/web`, () => {
    // Unfiltered, this installs apps/marketing's Remotion into the public image
    // — redistribution we hold no rights to pass on. See docs/third-party-licences.md.
    const install = runtimeStage
      .split(`\n`)
      .find((line) => line.startsWith(`RUN bun install`))
    expect(install).toBeDefined()
    expect(install).toContain(`--filter '@exp/web'`)
  })

  it(`still copies every workspace package.json into the runtime stage`, () => {
    // --frozen-lockfile validates the FULL workspace set regardless of --filter:
    // drop one of these COPYs and the install dies with "lockfile had changes".
    // Only the image build catches that, and it runs on master, not on the PR.
    for (const workspace of [`marketing`, `push-relay`, `steer-relay`]) {
      expect(runtimeStage).toContain(`apps/${workspace}/package.json`)
    }
  })
})

describe(`Remotion licence determinations`, () => {
  const pins = Object.entries({
    ...marketingPkg.dependencies,
    ...marketingPkg.devDependencies,
  }).filter(([name]) => name === `remotion` || name.startsWith(`@remotion/`))

  it(`records the exact version the determinations were made against`, () => {
    // The licence file's own header announces that the terms CHANGE in Remotion
    // 5.0 (remotion-dev/remotion#3750). A major bump is a re-audit — re-read the
    // terms, re-vendor docs/licences/remotion-LICENSE.txt, re-date the doc.
    expect(pins.length).toBeGreaterThan(0)
    const versions = new Set(pins.map(([, version]) => version))
    expect(versions.size, `all @remotion/* pins move together`).toBe(1)
    const [version] = [...versions]
    expect(version.startsWith(`4.`), `Remotion 5.0 changes the licence`).toBe(
      true,
    )
    expect(policy).toContain(version)
  })

  it(`names every Remotion package the marketing app pulls in`, () => {
    for (const [name] of pins) expect(policy).toContain(name)
  })

  it(`vendors the licence text npm does not publish`, () => {
    // remotion/package.json says "SEE LICENSE IN LICENSE.md" and the tarball
    // omits that file, so this copy is the only record of the terms we rely on.
    // Both clauses below are load-bearing: the first is why we owe no fee, the
    // second is why shipping it to third parties was never covered.
    expect(remotionLicence).toContain(
      `a for-profit organization with up to 3 employees`,
    )
    expect(remotionLicence).toContain(
      `for the purpose of selling, renting, licensing, relicensing, or sublicensing`,
    )
  })
})
