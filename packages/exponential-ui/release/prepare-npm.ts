#!/usr/bin/env bun
// VAPP-91: stages the SDK's npm packages for publishing. The workspace
// package.json files stay `"private": true` with `main` on the TS sources
// (the monorepo consumes the sources); this writes, per package, a staging
// directory with the built `dist/`, the data folders, LICENSE + NOTICE and a
// PUBLISHABLE package.json (version from the tag, `private` dropped, entry
// points on dist/, `workspace:*` pinned to the release version).
//
//   bun packages/exponential-ui/release/prepare-npm.ts <version> <outDir>
//   → <outDir>/ui, <outDir>/ui-react   (then `npm publish` / `npm pack` there)

import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { buildNpm } from "./build-npm"

const repo = resolve(import.meta.dir, `..`, `..`, `..`)

interface Spec {
  dir: string
  out: string
  files: string[]
  exports: Record<string, unknown>
}

const SPECS: Spec[] = [
  {
    dir: `packages/exponential-ui`,
    out: `ui`,
    files: [`dist`, `catalog`, `fixtures`, `themes`, `docs`, `vendor`, `generated`, `conformance`, `README.md`],
    exports: {
      ".": { types: `./dist/types/src/index.d.ts`, import: `./dist/index.js`, default: `./dist/index.js` },
      "./catalog/*": `./catalog/*`,
      "./fixtures/*": `./fixtures/*`,
      "./docs/*": `./docs/*`,
      "./vendor/*": `./vendor/*`,
      "./themes/*": `./themes/*`,
      "./generated/*": `./generated/*`,
      "./conformance/*": `./conformance/*`,
      "./package.json": `./package.json`,
    },
  },
  {
    dir: `packages/exponential-ui-react`,
    out: `ui-react`,
    files: [`dist`, `README.md`],
    exports: {
      ".": { types: `./dist/types/index.d.ts`, import: `./dist/index.js`, default: `./dist/index.js` },
      "./primitives": { types: `./dist/types/primitives/index.d.ts`, import: `./dist/primitives/index.js`, default: `./dist/primitives/index.js` },
      "./styles.css": `./dist/exponential-ui-react.css`,
      "./package.json": `./package.json`,
    },
  },
]

export async function prepareNpm(version: string, outDir: string): Promise<string[]> {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error(`not a semver version: ${version}`)
  const staged: string[] = []
  for (const spec of SPECS) {
    const dir = join(repo, spec.dir)
    await buildNpm(dir)
    if (spec.out === `ui-react`) {
      const { execSync } = await import(`node:child_process`)
      execSync(`bun run build:css`, { cwd: dir, stdio: `inherit` })
    }
    const target = join(outDir, spec.out)
    rmSync(target, { recursive: true, force: true })
    mkdirSync(target, { recursive: true })
    for (const f of spec.files) if (existsSync(join(dir, f))) cpSync(join(dir, f), join(target, f), { recursive: true, filter: (p) => !p.endsWith(`.test.ts`) && !p.includes(`/node_modules`) && !p.includes(`/conformance/fonts/`) && !p.endsWith(`/conformance/fonts`) })
    cpSync(join(repo, `LICENSE`), join(target, `LICENSE`))
    cpSync(join(repo, `NOTICE`), join(target, `NOTICE`))
    const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as Record<string, unknown> & { dependencies?: Record<string, string> }
    const deps = Object.fromEntries(Object.entries(pkg.dependencies ?? {}).map(([k, v]) => [k, v.startsWith(`workspace:`) ? `^${version}` : v]))
    const out: Record<string, unknown> = {
      name: pkg.name,
      version,
      description: pkg.description,
      license: `Apache-2.0`,
      homepage: `https://ui.exponential.at`,
      repository: { type: `git`, url: `git+https://github.com/Niach/exponential.git`, directory: spec.dir },
      keywords: [`a2ui`, `generative-ui`, `exponential-ui`],
      type: `module`,
      main: `./dist/index.js`,
      module: `./dist/index.js`,
      types: (spec.exports[`.`] as { types: string }).types,
      exports: spec.exports,
      files: [...spec.files, `LICENSE`, `NOTICE`],
      sideEffects: spec.out === `ui-react` ? [`*.css`] : false,
      ...(Object.keys(deps).length ? { dependencies: deps } : {}),
      ...(pkg.peerDependencies ? { peerDependencies: pkg.peerDependencies } : {}),
      publishConfig: { access: `public`, provenance: true },
    }
    writeFileSync(join(target, `package.json`), `${JSON.stringify(out, null, 2)}\n`)
    staged.push(target)
    console.log(`staged ${pkg.name}@${version} → ${target}`)
  }
  return staged
}

if (import.meta.main) {
  const [version, outDir] = process.argv.slice(2)
  if (!version || !outDir) {
    console.error(`usage: prepare-npm.ts <version> <outDir>`)
    process.exit(2)
  }
  await prepareNpm(version, resolve(outDir))
}
