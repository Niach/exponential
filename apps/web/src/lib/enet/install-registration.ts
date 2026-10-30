// EXP-1457 step 1 / VAPP-37: what a browser install sends to
// `installs.register`. The wire contract lives in lib/trpc/installs.ts: the
// install's eNet identity signs the UTF-8 string
// `exp-install-v1\n<userId>\n<endpointId>\n<x25519Public>\n<signedAt>`;
// endpoint ids are lowercase hex, keys and signatures unpadded base64url.
// Pure (no wasm import): the eNet calls arrive as `InstallCrypto`.

/** The eNet calls registration needs (exponential-net-wasm's free fns). */
export type InstallCrypto = {
  /** The X25519 public key `PushKeypair::derive` gives for the identity. */
  pushPublicKey: (identity: Uint8Array) => Uint8Array
  /** ed25519 signature (64 bytes) by the identity over `message`. */
  identitySign: (identity: Uint8Array, message: Uint8Array) => Uint8Array
}

export type InstallRegistration = {
  kind: `web`
  label: string
  platform: string
  endpointId: string
  x25519Public: string
  signedAt: string
  signature: string
}

const ENDPOINT_ID_RE = /^[0-9a-f]{64}$/

/** The string the identity signs; byte-equal to the server's `installMessage`. */
export function installMessage(
  userId: string,
  endpointId: string,
  x25519Public: string,
  signedAt: string
): string {
  return `exp-install-v1\n${userId}\n${endpointId}\n${x25519Public}\n${signedAt}`
}

export function base64url(bytes: Uint8Array): string {
  let binary = ``
  for (const b of bytes) binary += String.fromCharCode(b)
  return btoa(binary).replace(/\+/g, `-`).replace(/\//g, `_`).replace(/=+$/, ``)
}

/** A signed `installs.register` input for this browser install. */
export function buildRegistration(args: {
  userId: string
  identity: Uint8Array
  endpointId: string
  crypto: InstallCrypto
  now?: Date
  userAgent?: string
}): InstallRegistration {
  const endpointId = args.endpointId.toLowerCase()
  if (!ENDPOINT_ID_RE.test(endpointId)) throw new Error(`endpoint id is not 32-byte hex`)
  const x25519Public = base64url(args.crypto.pushPublicKey(args.identity))
  const signedAt = (args.now ?? new Date()).toISOString()
  const message = installMessage(args.userId, endpointId, x25519Public, signedAt)
  const signature = args.crypto.identitySign(args.identity, new TextEncoder().encode(message))
  if (signature.length !== 64) throw new Error(`ed25519 signature must be 64 bytes`)
  const { label, platform } = browserLabel(args.userAgent ?? globalThis.navigator?.userAgent ?? ``)
  return {
    kind: `web`,
    label,
    platform,
    endpointId,
    x25519Public,
    signedAt,
    signature: base64url(signature),
  }
}

/** "Chrome on macOS" + the OS as the platform, from a user agent. */
export function browserLabel(userAgent: string): { label: string; platform: string } {
  const browser = /Edg\//.test(userAgent)
    ? `Edge`
    : /Firefox\//.test(userAgent)
      ? `Firefox`
      : /Chrome\//.test(userAgent)
        ? `Chrome`
        : /Safari\//.test(userAgent)
          ? `Safari`
          : `Browser`
  const platform = /iPhone|iPad/.test(userAgent)
    ? `iOS`
    : /Android/.test(userAgent)
      ? `Android`
      : /Mac OS X|Macintosh/.test(userAgent)
        ? `macOS`
        : /Windows/.test(userAgent)
          ? `Windows`
          : /Linux/.test(userAgent)
            ? `Linux`
            : `web`
  return { label: platform === `web` ? browser : `${browser} on ${platform}`, platform }
}
