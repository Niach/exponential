#!/usr/bin/env bun
// VAPP-91: until the npm packages are published, the samples install the
// SDK the way a registry would hand it out: staged by the release script and
// packed to .packs/ (gitignored). After the first publish, replace the
// `file:` deps in web/package.json with "^<version>" and drop this step.
//
//   bun samples/exponential-ui/pack-local.ts

import { execSync } from "node:child_process"
import { mkdirSync, rmSync } from "node:fs"
import { join } from "node:path"
import { prepareNpm } from "../../packages/exponential-ui/release/prepare-npm"
import uiPackage from "../../packages/exponential-ui/package.json"

const packs = join(import.meta.dir, `.packs`)
const stage = join(packs, `.stage`)
rmSync(packs, { recursive: true, force: true })
mkdirSync(packs, { recursive: true })
for (const dir of await prepareNpm(uiPackage.version, stage)) execSync(`npm pack --pack-destination ${packs}`, { cwd: dir, stdio: `inherit` })
rmSync(stage, { recursive: true, force: true })
