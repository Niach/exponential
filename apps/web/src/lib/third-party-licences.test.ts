// EXP-380 — gates for the non-OSS corner of the dependency graph.
//
// Remotion is source-available, not open source. It is licensed to US on the
// basis of our headcount, it has no sublicence clause, and its npm tarball ships
// no LICENSE.md at all. docs/third-party-licences.md records what we are and are
// not permitted to do with it; this file stops those determinations from going
// quietly stale. Sibling of icons.test.ts, which locks the icon registry the
// same way — read repo files, byte-compare, fail loudly.
//
// REV-2 widened it from the web Dockerfile to EVERY image the repo builds: the
// steer relay image is published just as publicly and carried the same 178M
// Remotion tree for as long as this gate only read one file.

import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const read = (path: string) => readFileSync(join(repoRoot, path), `utf8`)

const policy = read(`docs/third-party-licences.md`)
const remotionLicence = read(`docs/licences/remotion-LICENSE.txt`)
const marketingPkg = JSON.parse(read(`apps/marketing/package.json`)) as {
  dependencies: Record<string, string>
  devDependencies: Record<string, string>
}

/* Every image this repo builds, and the ONE workspace each is allowed to install.
   Remotion lives in apps/marketing, so an unfiltered `bun install` in any stage
   whose node_modules survives into the final image redistributes it (REV-2).

   `copiesBuilderNodeModules` is what decides WHICH installs are published, and
   it is asserted rather than trusted:
     - false (the web image) — the builder installs the whole workspace on
       purpose (the vite build reaches across it) and is discarded; only
       apps/web/.output comes out. The gated install is the runtime stage's own.
     - true (the relays) — they have no build step and no second install, so the
       runtime stage copies the builder's node_modules wholesale. EVERY install
       in the file is published, so every one must be filtered. */
const IMAGES = [
  { file: `Dockerfile`, workspace: `@exp/web`, copiesBuilderNodeModules: false },
  {
    file: `Dockerfile.steer-relay`,
    workspace: `@exp/steer-relay`,
    copiesBuilderNodeModules: true,
  },
  {
    file: `Dockerfile.push-relay`,
    workspace: `@exp/push-relay`,
    copiesBuilderNodeModules: true,
  },
] as const

describe.each(IMAGES)(
  `$file dependency scope`,
  ({ file, workspace, copiesBuilderNodeModules }) => {
    const dockerfile = read(file)
    /* The runtime stage is everything after the LAST `FROM` — that is the layer
       set that becomes the published image. */
    const runtimeStage = dockerfile.slice(dockerfile.lastIndexOf(`\nFROM `))
    const installs = (copiesBuilderNodeModules ? dockerfile : runtimeStage)
      .split(`\n`)
      .filter((line) => line.startsWith(`RUN bun install`))

    it(`ships the node_modules provenance this gate assumes`, () => {
      // Flip this and the wrong installs get gated above, silently. Changing the
      // constant is the point: it forces re-reading which install is published.
      expect(runtimeStage.includes(`COPY --from=builder /app/node_modules`)).toBe(
        copiesBuilderNodeModules,
      )
    })

    it(`scopes every published install to ${workspace}`, () => {
      // Unfiltered, these install apps/marketing's Remotion into the image —
      // redistribution we hold no rights to pass on. See docs/third-party-licences.md.
      expect(installs.length).toBeGreaterThan(0)
      for (const install of installs)
        expect(install).toContain(`--filter '${workspace}'`)
    })

    it(`still copies every workspace package.json into the installing stage`, () => {
      // --frozen-lockfile validates the FULL workspace set regardless of --filter:
      // drop one of these COPYs and the install dies with "lockfile had changes".
      // Only the image build catches that, and it runs on master, not on the PR.
      for (const other of [`marketing`, `push-relay`, `steer-relay`]) {
        expect(dockerfile).toContain(`apps/${other}/package.json`)
      }
    })

    it(`carries the Apache-2.0 LICENSE and NOTICE`, () => {
      // EXP-376: section 4(a)/4(d) are owed to every recipient of the image.
      expect(runtimeStage).toContain(`LICENSE`)
      expect(runtimeStage).toContain(`NOTICE`)
    })
  },
)

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
