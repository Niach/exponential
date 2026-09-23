// EXP-630: the Linear entry in the source registry (lib/import/sources.ts).
// The credential is the pasted personal API key; it never leaves this
// adapter (the applier only sees `fetchAsset(ref)`).
import { LinearApiError, LinearClient } from "@/lib/import/linear/client"
import {
  fetchLinearSnapshot,
  linearSnapshotSchema,
  VIEWER_QUERY,
} from "@/lib/import/linear/snapshot"
import {
  LINEAR_SOURCE,
  LINEAR_SOURCE_LABEL,
  linearPreview,
  toLinearBundle,
} from "@/lib/import/linear/bundle"
import { fetchLinearAsset, probeLinearAssetSize } from "@/lib/import/linear/assets"
import type { ImportSource } from "@/lib/import/source-types"

export const LINEAR_API_KEY_PATTERN = /^lin_api_[A-Za-z0-9]{20,}$/

export const linearImportSource: ImportSource = {
  id: LINEAR_SOURCE,
  label: LINEAR_SOURCE_LABEL,

  async validateCredential(credential) {
    const key = credential.trim()
    if (!LINEAR_API_KEY_PATTERN.test(key)) {
      return {
        ok: false,
        reason: `That does not look like a Linear personal API key (they start with lin_api_).`,
      }
    }
    try {
      const client = new LinearClient(key, { maxAttempts: 2 })
      const data = await client.query<{
        viewer: { name: string; email: string }
        organization: { name: string }
      }>(VIEWER_QUERY)
      return { ok: true, who: `${data.viewer.email} in ${data.organization.name}` }
    } catch (err) {
      return {
        ok: false,
        reason: err instanceof LinearApiError ? err.message : `Could not reach Linear.`,
      }
    }
  },

  async discover({ credential, onProgress }) {
    if (!credential) {
      throw new Error(`The Linear API key for this import has expired. Connect again.`)
    }
    const key = credential.trim()
    const client = new LinearClient(key)
    const snapshot = await fetchLinearSnapshot(client, {
      onProgress: onProgress ? async ({ done, total }) => onProgress(done, total) : undefined,
      probeAssetSize: (url) => probeLinearAssetSize(key, url),
    })
    return { payload: snapshot, preview: linearPreview(snapshot) }
  },

  toBundle(payload, plan) {
    const snapshot = linearSnapshotSchema.parse(payload)
    return toLinearBundle(snapshot, { routing: plan.routing })
  },

  async fetchAsset({ credential, ref }) {
    if (!credential) return null
    return fetchLinearAsset(credential.trim(), ref)
  },
}
