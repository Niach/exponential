import { describe, expect, it } from "vitest"
import {
  androidPasskeyOrigins,
  buildAppleAppSiteAssociation,
  buildAssetLinks,
  parseFingerprints,
} from "./app-links"

// The association payloads behind /.well-known/apple-app-site-association and
// /.well-known/assetlinks.json (EXP-92). Path patterns must keep matching the
// canonical link shapes (buildIssueDeepLinkPath + /invite/$token) — Apple's
// CDN caches the AASA, so mistakes here are slow to roll back.

describe(`buildAppleAppSiteAssociation`, () => {
  it(`claims exactly the issue-detail and invite paths for both app ids`, () => {
    const aasa = buildAppleAppSiteAssociation() as {
      applinks: {
        details: { appIDs: string[]; components: { "/": string }[] }[]
      }
    }
    expect(aasa.applinks.details).toHaveLength(1)
    const detail = aasa.applinks.details[0]
    expect(detail.appIDs).toEqual([
      `V6W7BVCSM8.at.exponential`,
      `V6W7BVCSM8.at.exponential.staging`,
    ])
    expect(detail.components).toEqual([
      { "/": `/t/*/boards/*/issues/*` },
      { "/": `/invite/*` },
    ])
  })

  it(`associates the same app ids for passkeys (webcredentials, EXP-857)`, () => {
    const aasa = buildAppleAppSiteAssociation() as {
      webcredentials: { apps: string[] }
    }
    expect(aasa.webcredentials.apps).toEqual([
      `V6W7BVCSM8.at.exponential`,
      `V6W7BVCSM8.at.exponential.staging`,
    ])
  })
})

describe(`androidPasskeyOrigins`, () => {
  it(`turns a colon-hex cert fingerprint into the apk-key-hash origin`, () => {
    // sha256 = 00..ff pattern; base64url of those 32 bytes, no padding.
    const hex = Array.from({ length: 32 }, (_, i) =>
      i.toString(16).padStart(2, `0`).toUpperCase()
    ).join(`:`)
    const expected = Buffer.from(hex.replace(/:/g, ``), `hex`).toString(
      `base64url`
    )
    expect(androidPasskeyOrigins([hex])).toEqual([
      `android:apk-key-hash:${expected}`,
    ])
  })

  it(`drops fingerprints that are not a 32-byte hex digest`, () => {
    expect(androidPasskeyOrigins([`AA:BB`, ``])).toEqual([])
  })
})

describe(`parseFingerprints`, () => {
  it(`splits, trims, uppercases, and drops empties`, () => {
    expect(parseFingerprints(` aa:bb:cc , DD:EE:FF ,, `)).toEqual([
      `AA:BB:CC`,
      `DD:EE:FF`,
    ])
  })

  it(`returns empty for unset input`, () => {
    expect(parseFingerprints(undefined)).toEqual([])
    expect(parseFingerprints(``)).toEqual([])
  })
})

describe(`buildAssetLinks`, () => {
  it(`returns null with no fingerprints (route 404s)`, () => {
    expect(buildAssetLinks([])).toBeNull()
  })

  it(`emits one handle_all_urls statement per package with all fingerprints`, () => {
    const statements = buildAssetLinks([`AA:BB`, `CC:DD`]) as {
      relation: string[]
      target: {
        namespace: string
        package_name: string
        sha256_cert_fingerprints: string[]
      }
    }[]
    expect(statements.map((s) => s.target.package_name)).toEqual([
      `at.exponential`,
      `at.exponential.staging`,
    ])
    for (const s of statements) {
      expect(s.relation).toEqual([
        `delegate_permission/common.handle_all_urls`,
        `delegate_permission/common.get_login_creds`,
      ])
      expect(s.target.namespace).toBe(`android_app`)
      expect(s.target.sha256_cert_fingerprints).toEqual([`AA:BB`, `CC:DD`])
    }
  })
})
