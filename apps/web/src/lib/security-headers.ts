// The response-hardening header set the Bun server stamps on every response
// (`server-bun.ts`, gated by SECURITY_HEADERS_ENABLED — baked to 'true' in the
// published image, which is cloud, staging AND the self-host distribution).
// Pure so every deploy shape is a test; `server-bun.ts` itself imports nitro
// internals and cannot load under vitest.

export interface SecurityHeadersEnv {
  /** `CLOUD_INSTANCE` — the opt-in cloud marker (`'true'`). */
  CLOUD_INSTANCE?: string
  /** `BETTER_AUTH_URL` — the app's public base URL. */
  BETTER_AUTH_URL?: string
}

/** True when the configured public base is plain `http://` (the no-domain LAN
 * self-host). Unset or unparsable = https posture. */
function isPlainHttpBase(base: string | undefined): boolean {
  if (!base) return false
  try {
    return new URL(base).protocol === `http:`
  } catch {
    return false
  }
}

// Conservative CSP that allows TanStack Start's inline hydration script,
// Google OAuth redirects and Electric long-poll requests against the same
// origin. Tightening to nonce-based scripts requires a Start-internal change
// and is left as follow-up. The dogfood feedback widget is cloud-only and
// same-origin ('self' covers it); self-hosted instances redirect to the cloud
// feedback board instead of embedding, so no external script-src entry is
// needed.
export function buildSecurityHeaders(
  env: SecurityHeadersEnv
): Record<string, string> {
  const cloud = env.CLOUD_INSTANCE === `true`
  // A plain-http LAN self-host runs the steer relay on `ws://<host>/steer`
  // (INSTALL.md); `wss:` alone would block the live-steer socket there.
  const connectSrc = isPlainHttpBase(env.BETTER_AUTH_URL)
    ? `connect-src 'self' https: wss: ws:`
    : `connect-src 'self' https: wss:`
  return {
    "Content-Security-Policy": [
      `default-src 'self'`,
      `script-src 'self' 'unsafe-inline'`,
      `style-src 'self' 'unsafe-inline' https://fonts.googleapis.com`,
      // Avatars come from wherever the IdP hosts them (Google, an OIDC
      // provider, Gravatar), so any https image is allowed.
      `img-src 'self' data: blob: https:`,
      `font-src 'self' data: https://fonts.gstatic.com`,
      connectSrc,
      `frame-src 'self' https://accounts.google.com`,
      `frame-ancestors 'none'`,
      `base-uri 'self'`,
      `form-action 'self' https://accounts.google.com`,
    ].join(`; `),
    "Referrer-Policy": `strict-origin-when-cross-origin`,
    "X-Content-Type-Options": `nosniff`,
    "X-Frame-Options": `SAMEORIGIN`,
    // `includeSubDomains` is cloud-only: on a self-host sharing an apex with
    // other services it would pin EVERY sibling subdomain to https for two
    // years, and HSTS cannot be revoked from the server side.
    "Strict-Transport-Security": cloud
      ? `max-age=63072000; includeSubDomains`
      : `max-age=63072000`,
  }
}
