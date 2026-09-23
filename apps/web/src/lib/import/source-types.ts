// EXP-630: what an adapter provides. Kept apart from sources.ts (the
// registry) so adapters can import the interface without pulling the
// registry — and the registry — back in.
import type { ImportBundle, ImportPlan, ImportPreview } from "@/lib/import/bundle"
import type { FetchedAsset } from "@/lib/import/apply"

export interface ImportSource {
  id: string
  label: string
  // Cheap credential check before a job is created.
  validateCredential(
    credential: string
  ): Promise<{ ok: true; who: string } | { ok: false; reason: string }>
  // The full fetch: everything the plan and the apply need, stored as jsonb.
  discover(args: {
    credential: string | null
    payload: unknown
    onProgress?: (done: number, total: number) => Promise<void>
  }): Promise<{ payload: unknown; preview: ImportPreview }>
  toBundle(payload: unknown, plan: Pick<ImportPlan, `routing` | `importArchived`>): ImportBundle
  fetchAsset(args: { credential: string | null; ref: string }): Promise<FetchedAsset | null>
}
