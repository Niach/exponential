// #EXP-1491: eMCP's boot check and daily passes, started once from
// server-bun.ts: the vault (a malformed VAULT_KEY stops the boot; the sweep
// migrates legacy and plaintext values and finishes rotations) and the
// `mcp_calls` retention.
import { sweepMcpAudit } from "@/lib/emcp/audit"
import { assertVaultConfigured } from "@/lib/emcp/vault"
import { sweepVault } from "@/lib/emcp/vault-sweep"

const INITIAL_DELAY_MS = 2 * 60 * 1000
const DAY_MS = 24 * 60 * 60 * 1000

let started = false

export function startEmcpSchedulers(): void {
  if (started) return
  started = true
  const { kid, derived } = assertVaultConfigured()
  if (derived) {
    console.warn(
      `[emcp-vault] VAULT_KEY is not set: sealing under the key derived from BETTER_AUTH_SECRET (kid ${kid}). Set VAULT_KEY=<kid>:<base64 of 32 bytes> for a key you can rotate.`
    )
  }
  setTimeout(() => {
    void sweepVault().then(sweepMcpAudit)
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweepVault().then(sweepMcpAudit)
  }, DAY_MS)
}
