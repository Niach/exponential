// EXP-1604: bundles motion-islands.client.ts (the page's Motion script) into
// one inline IIFE; build.ts and serve.ts pass it to `renderHtml`.

import path from "node:path"

export async function bundleMotionIslands(): Promise<string> {
  const result = await Bun.build({
    entrypoints: [path.join(import.meta.dir, `motion-islands.client.ts`)],
    target: `browser`,
    format: `esm`,
    minify: true,
  })
  if (!result.success) throw new Error(result.logs.map((l) => l.message).join(`\n`))
  const js = result.outputs.find((o) => o.kind === `entry-point`)
  if (!js) throw new Error(`motion islands: no bundle`)
  return await js.text()
}
