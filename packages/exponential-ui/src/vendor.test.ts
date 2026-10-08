// VAPP-85: the vendored A2UI v0.9 files are byte-pinned (the README's table)
// and the json-render attribution is recorded where the licence gate reads.

import { describe, expect, test } from "bun:test"
import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { A2UI_BASIC_CATALOG_ID, A2UI_VERSION } from "./catalog"

const pkgRoot = join(import.meta.dir, `..`)
const sha = (rel: string) => createHash(`sha256`).update(readFileSync(join(pkgRoot, rel))).digest(`hex`)

const PINS: Record<string, string> = {
  "vendor/a2ui/v0_9/basic_catalog.json": `3dece1107de5cd9856cebf882f9e2c01ca3e2a5eba209b8ed585688765281e10`,
  "vendor/a2ui/v0_9/common_types.json": `899a9307d80fd9db71f640aec518f74defbe6dcd104f76d0e02eb83b79ea9f89`,
  "vendor/a2ui/v0_9/server_to_client.json": `e8c0d280ff9e338ceb26551cbf01cabeb50690f5ac941a27fef2ec0490682af2`,
  "vendor/a2ui/v0_9/client_to_server.json": `33f5aabc526f8a812a704c742832204b73b831105e073700e035d707dc7495cc`,
  "vendor/a2ui/v0_9/client_capabilities.json": `1178ec350a87313a49ced9dd00dd36a34fa3129fd67277091cecabb141b3ab84`,
  "vendor/a2ui/v0_9/basic_catalog_rules.txt": `ccd5b8158b7d459c26bb92dbe63fcb5d01a78311d39200951c5b062499b854df`,
  "vendor/a2ui/LICENSE.txt": `f9946796a8a5bd5981565142fa42e3b0cc36aef6f80e86beaf5905b3c0fef1b5`,
  "vendor/json-render/LICENSE.txt": `014bb31e83d5c2e76aea1cc6e82217346ab41362f32cb355ad0f5c10aa0aeaff`,
}

describe(`vendored sources`, () => {
  for (const [rel, digest] of Object.entries(PINS)) {
    test(`${rel} is byte-identical to upstream`, () => {
      expect(sha(rel)).toBe(digest)
      expect(readFileSync(join(pkgRoot, `vendor/a2ui/README.md`), `utf8`).includes(digest) || rel.includes(`json-render`)).toBe(true)
    })
  }

  test(`the pinned basic catalog is the v0.9 spec the map targets`, () => {
    const basic = JSON.parse(readFileSync(join(pkgRoot, `vendor/a2ui/v0_9/basic_catalog.json`), `utf8`)) as { catalogId: string; components: object; functions: object }
    expect(basic.catalogId).toBe(A2UI_BASIC_CATALOG_ID)
    expect(Object.keys(basic.components)).toHaveLength(18)
    expect(Object.keys(basic.functions)).toHaveLength(14)
    const messages = readFileSync(join(pkgRoot, `vendor/a2ui/v0_9/server_to_client.json`), `utf8`)
    expect(messages).toContain(`"const": "${A2UI_VERSION}"`)
  })

  test(`both licences are Apache-2.0 and recorded in docs/third-party-licences.md`, () => {
    for (const rel of [`vendor/a2ui/LICENSE.txt`, `vendor/json-render/LICENSE.txt`])
      expect(readFileSync(join(pkgRoot, rel), `utf8`)).toContain(`Apache License`)
    const policy = readFileSync(join(pkgRoot, `..`, `..`, `docs/third-party-licences.md`), `utf8`)
    expect(policy).toContain(`packages/exponential-ui/vendor/a2ui`)
    expect(policy).toContain(`packages/exponential-ui/vendor/json-render`)
    expect(policy).toContain(`19919ef4c8ad3185867f70386fa4669284d7714c`)
    expect(policy).toContain(`fc2a696a50a30cb30c878ab1eb65e102487eea0f`)
  })
})
