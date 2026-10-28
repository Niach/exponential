// EXP-380 — every published image installs only its own workspace.
//
// docs/third-party-licences.md ("Published images install only their own
// workspace") records why: an unfiltered install hands every other workspace's
// dependencies to whoever pulls the image, and no notice covers them. This file
// stops that from going quietly stale. Sibling of icons.test.ts, which locks the
// icon registry the same way — read repo files, fail loudly.
//
// REV-2 widened it from the web Dockerfile to EVERY image the repo builds: the
// steer relay image is published just as publicly.

import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const read = (path: string) => readFileSync(join(repoRoot, path), `utf8`)

/* Every image this repo builds, and the ONE workspace each is allowed to install.
   An unfiltered `bun install` in any stage whose node_modules survives into the
   final image redistributes every other workspace's dependencies (REV-2).

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
    /* The scope whose install is published: the relays copy the builder's
       node_modules, the web image installs in the runtime stage itself. */
    const installScope = copiesBuilderNodeModules ? dockerfile : runtimeStage
    const installs = installScope
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
      // Unfiltered, these install every other workspace's dependencies into the
      // image — components no notice covers. See docs/third-party-licences.md.
      expect(installs.length).toBeGreaterThan(0)
      for (const install of installs)
        expect(install).toContain(`--filter '${workspace}'`)
    })

    it(`still copies every workspace package.json into the installing stage`, () => {
      // --frozen-lockfile validates the FULL workspace set regardless of --filter:
      // drop one of these COPYs and the install dies with "lockfile had changes".
      // Only the image build catches that, and it runs on master, not on the PR.
      // Checked against the SAME scope the published install runs in — for the
      // web image that is the runtime stage, whose stubs arrive via builder COPY.
      for (const other of [`marketing`, `push-relay`, `steer-relay`, `styleguide`]) {
        expect(installScope).toContain(`apps/${other}/package.json`)
      }
    })

    it(`carries the Apache-2.0 LICENSE and NOTICE`, () => {
      // EXP-376: section 4(a)/4(d) are owed to every recipient of the image.
      // Must be an actual COPY into the runtime layer set, not a mention in prose.
      expect(runtimeStage).toMatch(/^COPY [^\n]*LICENSE[^\n]*NOTICE[^\n]*$/m)
    })
  },
)
