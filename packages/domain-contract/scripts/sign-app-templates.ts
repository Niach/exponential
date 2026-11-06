// VAPP-234 (fixtures/app-templates.json): sign the template gallery's files.
//
//   EXP_APP_TEMPLATES_SEED=<32 bytes, base64url> bun packages/domain-contract/scripts/sign-app-templates.ts
//   bun packages/domain-contract/scripts/sign-app-templates.ts --check
//
// Signing re-signs every `samples/apps/<id>.app.json` of the fixture's list
// in place (the package format's `author` + `signature`) and refuses a seed
// whose public key is not the fixture's pinned `signer.key`. `--check`
// verifies every file against that key (exit 1 on any failure).
//
// VAPP-271: `SIGNED_SAMPLES` are signed the same way by the same pinned
// signer but are NO gallery template (the fixture's `templates` and
// apps/web/src/lib/apps/templates.ts never list them): package files
// `{package, author, signature}` the bundler exports (the Photobook). Until
// one is signed, `--check` names it unsigned on stderr and does not fail; a
// sample signed by any other key, or one that does not verify, fails.

import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import fixture from "../fixtures/app-templates.json" with { type: "json" }
import { base64UrlDecode, signPackageFile, verifyPackageFile } from "../../exponential-ui/src/host/signature"

const root = join(import.meta.dir, `..`, `..`, `..`)
const pathOf = (id: string) => join(root, fixture.dir, `${id}${fixture.fileSuffix}`)

/** The signed samples outside the gallery, by id (`<dir>/<id><fileSuffix>`). */
export const SIGNED_SAMPLES: readonly string[] = [`photobook`]

async function problemsOf(id: string): Promise<{ problem: string | null; unsigned: boolean }> {
  const verified = await verifyPackageFile(JSON.parse(readFileSync(pathOf(id), `utf8`)))
  if (verified.status === `unsigned`) return { problem: `${id}: unsigned`, unsigned: true }
  if (verified.status !== `valid`) return { problem: `${id}: ${verified.issues.map((i) => `${i.path} ${i.message}`).join(`; `)}`, unsigned: false }
  if (verified.author.key !== fixture.signer.key) return { problem: `${id}: signed by ${verified.author.key}, not the pinned signer`, unsigned: false }
  return { problem: null, unsigned: false }
}

/** Every template's problems (empty = all signed by the pinned key). */
export async function checkAppTemplates(): Promise<string[]> {
  const problems: string[] = []
  for (const t of fixture.templates) {
    const { problem } = await problemsOf(t.id)
    if (problem) problems.push(problem)
  }
  return problems
}

/** The signed samples: their problems (a failure) and the ones still unsigned (no failure yet). */
export async function checkSignedSamples(): Promise<{ problems: string[]; unsigned: string[] }> {
  const problems: string[] = []
  const unsigned: string[] = []
  for (const id of SIGNED_SAMPLES) {
    const got = await problemsOf(id)
    if (got.unsigned) unsigned.push(id)
    else if (got.problem) problems.push(got.problem)
  }
  return { problems, unsigned }
}

if (import.meta.main) {
  if (process.argv.includes(`--check`)) {
    const problems = await checkAppTemplates()
    const samples = await checkSignedSamples()
    for (const p of [...problems, ...samples.problems]) console.error(p)
    for (const id of samples.unsigned) console.error(`${id}: unsigned (sign it with EXP_APP_TEMPLATES_SEED)`)
    process.exit(problems.length || samples.problems.length ? 1 : 0)
  }
  const seed = base64UrlDecode(process.env.EXP_APP_TEMPLATES_SEED ?? ``)
  if (seed?.length !== 32) throw new Error(`EXP_APP_TEMPLATES_SEED is 32 bytes, unpadded base64url`)
  for (const id of [...fixture.templates.map((t) => t.id), ...SIGNED_SAMPLES]) {
    const file = JSON.parse(readFileSync(pathOf(id), `utf8`)) as Record<string, unknown>
    const signed = await signPackageFile(file, { name: fixture.signer.name, seed })
    if (signed.author.key !== fixture.signer.key) throw new Error(`the seed's key ${signed.author.key} is not the pinned ${fixture.signer.key}`)
    writeFileSync(pathOf(id), `${JSON.stringify(signed, null, 2)}\n`)
    console.log(`signed ${fixture.dir}/${id}${fixture.fileSuffix}`)
  }
}
