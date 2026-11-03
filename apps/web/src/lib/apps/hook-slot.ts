import type { VaultSlot } from "@exp/emcp/vault"

/** #VAPP-237: a webhook secret's vault slot. */
export function hookSlot(id: string): VaultSlot {
  return { table: `app_hooks`, column: `secret_ciphertext`, rowId: id }
}
