import { describe, expect, it } from "vitest"
import { signPackageFile } from "@exponential_app/eui"
import { buildAppExport, appExportText } from "@exp/domain-contract/app-export"
import { checkAppImport } from "@/lib/trpc/apps"

// VAPP-161: a signed app file (the package format's author + signature)
// imports only when it verifies; an unsigned one imports as before.

const SEED = Uint8Array.from(Buffer.from(`9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60`, `hex`)) // RFC 8032 §7.1 TEST 1
const file = buildAppExport({
  app: { name: `Hello` },
  package: {
    id: `acme.hello`,
    name: `Hello`,
    version: `1.0.0`,
    catalogId: `https://ui.exponential.at/catalogs/core/v1`,
    sources: [],
    templates: { main: { components: [{ id: `root`, component: `Text`, text: `Hi` }] } },
  },
  includeRecords: false,
}) as unknown as Record<string, unknown>

describe(`checkAppImport: signatures`, () => {
  it(`imports an unsigned and a signed file`, async () => {
    expect((await checkAppImport(appExportText(file as never))).issues).toEqual([])
    const signed = await signPackageFile(file, { name: `Test Author`, seed: SEED })
    expect((await checkAppImport(JSON.stringify(signed))).issues).toEqual([])
  })

  it(`refuses a tampered or malformed signature`, async () => {
    const signed = await signPackageFile(file, { name: `Test Author`, seed: SEED })
    const tampered = { ...signed, app: { ...(signed.app as object), name: `Evil` } }
    expect(await checkAppImport(JSON.stringify(tampered))).toEqual({ issues: [{ path: `/signature`, message: `does not verify against /author/key` }], file: null })
    const malformed = { ...signed, signature: { alg: `ed25519`, value: `x` } }
    expect((await checkAppImport(JSON.stringify(malformed))).issues).toEqual([{ path: `/signature/value`, message: `value is 64 bytes, base64url` }])
  })
})
