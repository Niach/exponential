// App Review 2.5.4 rejected iOS 0.14.112 for the `audio` background mode
// (no persistent background audio feature). The iOS Info.plist is generated
// from apps/ios/Project.swift; this keeps `audio` out of UIBackgroundModes.

import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)

describe(`iOS Info.plist`, () => {
  it(`declares no audio background mode`, () => {
    const project = readFileSync(join(repoRoot, `apps/ios/Project.swift`), `utf8`)
    const modes = [...project.matchAll(/"UIBackgroundModes":\s*\.array\(\[([^\]]*)\]\)/g)]
    expect(modes.length).toBeGreaterThan(0)
    for (const [, list] of modes) {
      expect(list).not.toMatch(/"audio"/)
      expect(list).toMatch(/"remote-notification"/)
    }
  })
})
