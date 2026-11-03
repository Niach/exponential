// VAPP-234 (fixtures/app-templates.json): sign the template gallery's files.
//
//   EXP_APP_TEMPLATES_SEED=<32 bytes, base64url> bun packages/domain-contract/scripts/sign-app-templates.ts
//   bun packages/domain-contract/scripts/sign-app-templates.ts --check
//
// Signing re-signs every `samples/apps/<id>.app.json` of the fixture's list
// in place (the package format's `author` + `signature`) and refuses a seed
// whose public key is not the fixture's pinned `signer.key`. `--check`
// verifies every file against that key (exit 1 on any failure).

import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import fixture from "../fixtures/app-templates.json" with { type: "json" }
import { base64UrlDecode, signPackageFile, verifyPackageFile } from "../../exponential-ui/src/host/signature"

const root = join(import.meta.dir, `..`, `..`, `..`)
const pathOf = (id: string) => join(root, fixture.dir, `${id}${fixture.fileSuffix}`)

/** Every template's problems (empty = all signed by the pinned key). */
export async function checkAppTemplates(): Promise<string[]> {
  const problems: string[] = []
  for (const t of fixture.templates) {
    const verified = await verifyPackageFile(JSON.parse(readFileSync(pathOf(t.id), `utf8`)))
    if (verified.status !== `valid`) problems.push(`${t.id}: ${verified.status === `unsigned` ? `unsigned` : verified.issues.map((i) => `${i.path} ${i.message}`).join(`; `)}`)
    else if (verified.author.key !== fixture.signer.key) problems.push(`${t.id}: signed by ${verified.author.key}, not the pinned signer`)
  }
  return problems
}

if (import.meta.main) {
  if (process.argv.includes(`--check`)) {
    const problems = await checkAppTemplates()
    for (const p of problems) console.error(p)
    process.exit(problems.length ? 1 : 0)
  }
  const seed = base64UrlDecode(process.env.EXP_APP_TEMPLATES_SEED ?? ``)
  if (seed?.length !== 32) throw new Error(`EXP_APP_TEMPLATES_SEED is 32 bytes, unpadded base64url`)
  for (const t of fixture.templates) {
    const file = JSON.parse(readFileSync(pathOf(t.id), `utf8`)) as Record<string, unknown>
    const signed = await signPackageFile(file, { name: fixture.signer.name, seed })
    if (signed.author.key !== fixture.signer.key) throw new Error(`the seed's key ${signed.author.key} is not the pinned ${fixture.signer.key}`)
    writeFileSync(pathOf(t.id), `${JSON.stringify(signed, null, 2)}\n`)
    console.log(`signed ${fixture.dir}/${t.id}${fixture.fileSuffix}`)
  }
}
