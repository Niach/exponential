// Mobile app-link association payloads (EXP-92): iOS Universal Links + Android
// App Links open app.exponential.at issue/invite URLs in the native apps.
// Served from the two /.well-known routes; pure builders live here so they are
// unit-testable.

// Apple team + bundle ids (apps/ios/Project.swift). The AASA is served
// unconditionally on every instance — Apple only fetches it for domains listed
// in an app's associated-domains entitlement (app.exponential.at /
// next.exponential.at), so it is inert on self-hosted hosts.
const APPLE_APP_IDS = [
  `V6W7BVCSM8.at.exponential`,
  `V6W7BVCSM8.at.exponential.staging`,
]

// Android applicationIds (apps/android/app/build.gradle.kts flavors). Both
// packages ride every statement list — Digital Asset Links matches on the
// actual signing cert, so extra entries are harmless and one env var covers
// prod + staging hosts.
const ANDROID_PACKAGES = [`at.exponential`, `at.exponential.staging`]

// Only the link shapes the mobile apps can render. Claiming broader /t/*
// paths would hijack public feedback-board visitors and web-only surfaces.
const LINK_PATHS = [`/t/*/boards/*/issues/*`, `/invite/*`]

export function buildAppleAppSiteAssociation(): unknown {
  return {
    applinks: {
      details: [
        {
          appIDs: APPLE_APP_IDS,
          components: LINK_PATHS.map((path) => ({ "/": path })),
        },
      ],
    },
    // EXP-857: lets the iOS app run the passkey ceremony natively for this
    // host (ASAuthorizationPlatformPublicKeyCredentialProvider needs the
    // `webcredentials:<host>` association on both sides).
    webcredentials: { apps: APPLE_APP_IDS },
  }
}

/** Comma-separated colon-hex SHA-256 cert fingerprints → normalized list. */
export function parseFingerprints(raw: string | undefined): string[] {
  return (raw ?? ``)
    .split(`,`)
    .map((f) => f.trim().toUpperCase())
    .filter((f) => f.length > 0)
}

// Digital Asset Links statements. Fingerprints come from the env because the
// shipping cert is the Play App Signing key (Play Console → App integrity),
// which never touches the repo. Returns null when unset — the route 404s and
// Android link verification degrades to browser-open (also the self-hosted
// default posture).
export function buildAssetLinks(fingerprints: string[]): unknown[] | null {
  if (fingerprints.length === 0) return null
  return ANDROID_PACKAGES.map((packageName) => ({
    relation: [
      `delegate_permission/common.handle_all_urls`,
      // EXP-857: lets Credential Manager hand this host's passkeys to the app.
      `delegate_permission/common.get_login_creds`,
    ],
    target: {
      namespace: `android_app`,
      package_name: packageName,
      sha256_cert_fingerprints: fingerprints,
    },
  }))
}

// EXP-857: the WebAuthn origin Android's Credential Manager writes into
// clientDataJSON is not an https origin but
// `android:apk-key-hash:<base64url(sha256(signing cert DER))>` — the same
// SHA-256 the Digital Asset Links fingerprint spells as colon-hex. Derive the
// accepted origins from the ONE env var so the two stay in lockstep.
export function androidPasskeyOrigins(fingerprints: string[]): string[] {
  return fingerprints
    .map((fingerprint) => fingerprint.replace(/:/g, ``))
    .filter((hex) => /^[0-9A-F]{64}$/.test(hex))
    .map(
      (hex) => `android:apk-key-hash:${Buffer.from(hex, `hex`).toString(`base64url`)}`
    )
}
