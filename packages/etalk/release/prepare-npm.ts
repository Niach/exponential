#!/usr/bin/env bun
// EXP-1420: stages eTalk's contract as the npm package `@exponential_app/etalk`
// (the eUI pattern, packages/exponential-ui/release/prepare-npm.ts). The
// workspace package stays `@exp/etalk`, private, on its TS sources; this
// writes <outDir>/etalk with ONE ESM bundle per entry (the fixtures inlined,
// screen-memory.json from @exp/domain-contract included, so the package has
// no dependency), the declarations, the fixtures, LICENSE + NOTICE and a
// publishable package.json (version from the tag).
//
//   bun packages/etalk/release/prepare-npm.ts <version> <outDir>   (then `npm publish` there)

import { execSync } from "node:child_process"
import { cpSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { fixDeclarationSpecifiers } from "../../exponential-ui/release/build-npm"

const dir = resolve(import.meta.dir, `..`)
const repo = resolve(dir, `..`, `..`)
const ENTRIES = [`index`, `voice`, `voice-call`, `meeting`]

export async function prepareNpm(version: string, outDir: string): Promise<string> {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error(`not a semver version: ${version}`)
  const target = join(outDir, `etalk`)
  rmSync(target, { recursive: true, force: true })
  mkdirSync(target, { recursive: true })
  const result = await Bun.build({
    entrypoints: ENTRIES.map((e) => join(dir, `src`, `${e}.ts`)),
    outdir: join(target, `dist`),
    root: join(dir, `src`),
    target: `browser`,
    format: `esm`,
    splitting: true,
    sourcemap: `linked`,
    naming: { entry: `[name].js`, chunk: `chunks/[name]-[hash].js` },
  })
  if (!result.success) throw new AggregateError(result.logs, `bun build failed`)
  // rootDir is `packages/` (voice.ts reads a domain-contract fixture), so tsc
  // writes types/etalk/src: flatten to types/src and point the declarations'
  // fixture imports at the package's own fixtures/ (the only JSON they name),
  // with the attribute a NodeNext consumer requires.
  const types = join(target, `dist`, `types`)
  execSync(`bunx tsc -p tsconfig.build.json --outDir ${JSON.stringify(join(types, `tmp`))}`, { cwd: dir, stdio: `inherit` })
  renameSync(join(types, `tmp`, `etalk`, `src`), join(types, `src`))
  rmSync(join(types, `tmp`), { recursive: true, force: true })
  for (const f of readdirSync(join(types, `src`))) {
    const file = join(types, `src`, f)
    const text = readFileSync(file, `utf8`).replace(/from "\.\.\/fixtures\/([^"]+\.json)";/g, `from "../../../fixtures/$1" with { type: "json" };`)
    writeFileSync(file, text)
  }
  fixDeclarationSpecifiers(types)
  cpSync(join(dir, `fixtures`), join(target, `fixtures`), { recursive: true })
  for (const f of [`LICENSE`, `NOTICE`]) cpSync(join(repo, f), join(target, f))
  cpSync(join(dir, `README.md`), join(target, `README.md`))
  const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as { description?: string }
  const entry = (e: string) => ({ types: `./dist/types/src/${e}.d.ts`, import: `./dist/${e}.js`, default: `./dist/${e}.js` })
  const out = {
    name: `@exponential_app/etalk`,
    version,
    description: pkg.description,
    license: `Apache-2.0`,
    homepage: `https://exponential.at/talk/`,
    repository: { type: `git`, url: `git+https://github.com/Niach/exponential.git`, directory: `packages/etalk` },
    keywords: [`etalk`, `voice`, `speech`, `exponential`],
    type: `module`,
    main: `./dist/index.js`,
    module: `./dist/index.js`,
    types: `./dist/types/src/index.d.ts`,
    exports: {
      ".": entry(`index`),
      "./voice": entry(`voice`),
      "./voice-call": entry(`voice-call`),
      "./meeting": entry(`meeting`),
      "./fixtures/*": `./fixtures/*`,
      "./package.json": `./package.json`,
    },
    files: [`dist`, `fixtures`, `README.md`, `LICENSE`, `NOTICE`],
    sideEffects: false,
    publishConfig: { access: `public`, provenance: true },
  }
  writeFileSync(join(target, `package.json`), `${JSON.stringify(out, null, 2)}\n`)
  console.log(`staged ${out.name}@${version} → ${target}`)
  return target
}

if (import.meta.main) {
  const [version, outDir] = process.argv.slice(2)
  if (!version || !outDir) {
    console.error(`usage: prepare-npm.ts <version> <outDir>`)
    process.exit(2)
  }
  await prepareNpm(version, resolve(outDir))
}
