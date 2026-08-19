// Release-train gate (2026-08-19): every bun workspace member's package.json
// must be stubbed into each Docker builder stage. `bun install --frozen-lockfile`
// validates the FULL workspace set from bun.lock, so a new workspace that is
// missing from a Dockerfile's COPY list fails every image build with
// "@exp/<name>@workspace:* failed to resolve" — which is exactly what happened
// when EXP-551 added packages/emoji (web, steer-relay AND push-relay images all
// went red). Adding a workspace means adding a COPY line to all three.

import { describe, expect, it } from "vitest"
import { readFileSync, readdirSync, existsSync } from "node:fs"
import { join } from "node:path"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)

const DOCKERFILES = [`Dockerfile`, `Dockerfile.push-relay`, `Dockerfile.steer-relay`]

function workspaceManifests(): string[] {
  const root = JSON.parse(readFileSync(join(repoRoot, `package.json`), `utf8`)) as {
    workspaces: string[]
  }
  const out: string[] = []
  for (const pattern of root.workspaces) {
    const dir = pattern.replace(/\/\*$/, ``)
    for (const entry of readdirSync(join(repoRoot, dir), { withFileTypes: true })) {
      if (!entry.isDirectory()) continue
      const manifest = `${dir}/${entry.name}/package.json`
      if (existsSync(join(repoRoot, manifest))) out.push(manifest)
    }
  }
  return out.sort()
}

describe(`Dockerfile workspace stubs`, () => {
  const manifests = workspaceManifests()

  it(`discovers the workspace members`, () => {
    expect(manifests).toContain(`apps/web/package.json`)
    expect(manifests).toContain(`packages/emoji/package.json`)
  })

  for (const file of DOCKERFILES) {
    it(`${file} COPYs every workspace package.json before bun install`, () => {
      const text = readFileSync(join(repoRoot, file), `utf8`)
      const missing = manifests.filter(
        (m) => !text.includes(`COPY ${m} ${m}`),
      )
      expect(missing, `${file} is missing stub COPY lines for: ${missing.join(`, `)}`).toEqual([])
    })
  }
})
