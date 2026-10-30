// EXP-1459: the install's X25519 secret = `PushKeypair::derive(identity)`
// (enet-core push.rs; fixtures/push-seal.json `derive`): HKDF-SHA256 with no
// salt, info `exp/enet/push-x25519/1`, 32 bytes. Its public half is what
// `installs.register` published (`x25519Public`), so every tree key wrapped
// for this install opens with it. WebCrypto: an empty salt is the RFC 5869
// default (HashLen zero bytes), the same HMAC key.

const DERIVE_INFO = new TextEncoder().encode(`exp/enet/push-x25519/1`)

export async function deriveInstallSecret(identity: Uint8Array): Promise<Uint8Array> {
  if (identity.length !== 32) throw new Error(`identity must be 32 bytes`)
  const ikm = await crypto.subtle.importKey(`raw`, new Uint8Array(identity), `HKDF`, false, [`deriveBits`])
  const bits = await crypto.subtle.deriveBits(
    { name: `HKDF`, hash: `SHA-256`, salt: new Uint8Array(0), info: DERIVE_INFO },
    ikm,
    256
  )
  return new Uint8Array(bits)
}
