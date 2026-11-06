#!/usr/bin/env bun
// VAPP-263: stages the Engine's contract and TS twin as the npm package
// `@exponential_app/engine` (Voice's pattern, packages/etalk/release/prepare-npm.ts).
// The workspace package stays `@exp/eengine`, private, on its TS sources;
// this writes <outDir>/engine with ONE ESM bundle per entry (the fixtures
// and the private `@exp/emcp` / `@exp/domain-contract` code it uses inlined;
// genUI stays a dependency, `@exponential_app/eui`, at the minor this
// commit carries), the declarations (the private packages' ones carried
// along under dist/types), the fixtures, LICENSE + NOTICE and a publishable
// package.json (version from the tag). The audit hash uses `node:crypto`,
// so the package runs on Node 18+, Bun and Deno.
//
//   bun packages/eengine/release/prepare-npm.ts <version> <outDir>   (then `npm publish` there)

import { execSync } from "node:child_process"
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { dirname, join, relative, resolve } from "node:path"
import { fixDeclarationSpecifiers } from "../../exponential-ui/release/build-npm"

const dir = resolve(import.meta.dir, `..`)
const packages = resolve(dir, `..`)
const repo = resolve(packages, `..`)
const ENTRIES = [`index`, `capabilities`, `consent`, `lifecycle`, `providers`, `outbox`]
// The private workspace packages the twin imports, inlined in the bundle and
// carried along in the declarations: specifier → source file under packages/.
const PRIVATE: Record<string, string> = {
  "@exp/emcp": `emcp/src/index.ts`,
  "@exp/domain-contract/app-records": `domain-contract/src/app-records.ts`,
}

function walk(root: string): string[] {
  return readdirSync(root).flatMap((f) => {
    const p = join(root, f)
    return statSync(p).isDirectory() ? walk(p) : [p]
  })
}

export async function prepareNpm(version: string, outDir: string): Promise<string> {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error(`not a semver version: ${version}`)
  const target = join(outDir, `engine`)
  rmSync(target, { recursive: true, force: true })
  mkdirSync(target, { recursive: true })
  const eui = JSON.parse(readFileSync(join(packages, `exponential-ui`, `package.json`), `utf8`)) as { version: string }
  const result = await Bun.build({
    entrypoints: ENTRIES.map((e) => join(dir, `src`, `${e}.ts`)),
    outdir: join(target, `dist`),
    root: join(dir, `src`),
    target: `node`,
    format: `esm`,
    splitting: true,
    sourcemap: `linked`,
    external: [`@exponential_app/eui`, `@exponential_app/eui/*`],
    naming: { entry: `[name].js`, chunk: `chunks/[name]-[hash].js` },
  })
  if (!result.success) throw new AggregateError(result.logs, `bun build failed`)

  // Declarations: rootDir `packages/`, the private packages mapped to their
  // sources so tsc emits them too (types/<pkg>/src/…); then every `@exp/…`
  // specifier points at its emitted file and every JSON a declaration names
  // is copied next to it, with the attribute a NodeNext consumer requires.
  const types = join(target, `dist`, `types`)
  // Next to the package's own config: tsc finds `@types/bun` from there.
  const tsconfig = join(dir, `tsconfig.npm-types.json`)
  writeFileSync(
    tsconfig,
    JSON.stringify({
      extends: join(dir, `tsconfig.build.json`),
      include: [join(dir, `src/**/*.ts`)],
      exclude: [join(dir, `src/**/*.test.ts`)],
      compilerOptions: {
        rootDir: packages,
        outDir: types,
        baseUrl: dir,
        paths: Object.fromEntries(Object.entries(PRIVATE).map(([spec, file]) => [spec, [join(packages, file)]])),
      },
    })
  )
  try {
    execSync(`bunx tsc -p ${JSON.stringify(tsconfig)}`, { cwd: dir, stdio: `inherit` })
  } finally {
    rmSync(tsconfig)
  }
  for (const file of walk(types).filter((f) => f.endsWith(`.d.ts`))) {
    const source = join(packages, relative(types, dirname(file)))
    let text = readFileSync(file, `utf8`)
    for (const [spec, src] of Object.entries(PRIVATE)) {
      let rel = relative(dirname(file), join(types, src.replace(/\.ts$/, `.js`)))
      if (!rel.startsWith(`.`)) rel = `./${rel}`
      text = text.replaceAll(`from "${spec}"`, `from "${rel}"`)
    }
    text = text.replace(/from "(\.{1,2}\/[^"]+\.json)";/g, (_, spec: string) => {
      const json = resolve(source, spec)
      if (!existsSync(json)) throw new Error(`${file} names ${spec}, which ${json} is not`)
      const copy = join(types, relative(packages, json))
      mkdirSync(dirname(copy), { recursive: true })
      cpSync(json, copy)
      return `from "${spec}" with { type: "json" };`
    })
    writeFileSync(file, text)
  }
  fixDeclarationSpecifiers(types)
  const leaked = walk(types).filter((f) => f.endsWith(`.d.ts`) && /from "@exp\//.test(readFileSync(f, `utf8`)))
  if (leaked.length) throw new Error(`declarations still name a private package: ${leaked.join(`, `)}`)

  cpSync(join(dir, `fixtures`), join(target, `fixtures`), { recursive: true })
  for (const f of [`LICENSE`, `NOTICE`]) cpSync(join(repo, f), join(target, f))
  cpSync(join(dir, `README.md`), join(target, `README.md`))
  const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as { description?: string }
  const entry = (e: string) => ({ types: `./dist/types/eengine/src/${e}.d.ts`, import: `./dist/${e}.js`, default: `./dist/${e}.js` })
  const out = {
    name: `@exponential_app/engine`,
    version,
    description: pkg.description,
    license: `Apache-2.0`,
    homepage: `https://exponential.at/sdk/apps/`,
    repository: { type: `git`, url: `git+https://github.com/Niach/exponential.git`, directory: `packages/eengine` },
    keywords: [`engine`, `apps`, `genui`, `exponential`],
    type: `module`,
    main: `./dist/index.js`,
    module: `./dist/index.js`,
    types: `./dist/types/eengine/src/index.d.ts`,
    exports: {
      ".": entry(`index`),
      ...Object.fromEntries(ENTRIES.filter((e) => e !== `index`).map((e) => [`./${e}`, entry(e)])),
      "./fixtures/*": `./fixtures/*`,
      "./package.json": `./package.json`,
    },
    files: [`dist`, `fixtures`, `README.md`, `LICENSE`, `NOTICE`],
    // genUI's minor is its breaking line (0.x): any patch of this commit's minor.
    dependencies: { "@exponential_app/eui": `^${eui.version.replace(/\.\d+(-.*)?$/, `.0`)}` },
    engines: { node: `>=18` },
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
