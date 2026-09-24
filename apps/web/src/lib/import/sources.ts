// EXP-630: the adapter registry. A source knows how to turn a credential
// into a stored `payload` + `ImportPreview` (discovery), how to turn that
// payload into an `ImportBundle` under the plan's routing, and how to fetch
// the assets its bundle references. The core never imports an adapter by
// name outside this file.
import { importBundleSchema } from "@/lib/import/bundle"
import { IMPORT_MAX_ASSET_BYTES } from "@/lib/import/limits"
import { previewFromBundle } from "@/lib/import/preview"
import type { ImportSource } from "@/lib/import/source-types"
import { linearImportSource } from "@/lib/import/linear/source"

export type { ImportSource } from "@/lib/import/source-types"
export { previewFromBundle } from "@/lib/import/preview"

// The server fetches bundle asset refs itself, so a bundle must not be able
// to point it at the private network (SSRF): https only, no literal IPs, no
// loopback/link-local/RFC1918 names.
export function isSafePublicUrl(ref: string): boolean {
  let url: URL
  try {
    url = new URL(ref)
  } catch {
    return false
  }
  if (url.protocol !== `https:`) return false
  const host = url.hostname.toLowerCase()
  if (host === `localhost` || host.endsWith(`.localhost`) || host.endsWith(`.local`)) {
    return false
  }
  if (/^\[?[0-9a-f:]+\]?$/.test(host) && host.includes(`:`)) return false
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(host)) return false
  return true
}

// `imports.ingest`: the payload IS a bundle. Assets are plain public URLs.
export const bundleImportSource: ImportSource = {
  id: `bundle`,
  label: `Bundle`,
  async validateCredential() {
    return { ok: true, who: `bundle` }
  },
  async discover({ payload }) {
    const bundle = importBundleSchema.parse(payload)
    return { payload: bundle, preview: previewFromBundle(bundle) }
  },
  toBundle(payload) {
    return importBundleSchema.parse(payload)
  },
  async fetchAsset({ ref }) {
    if (!isSafePublicUrl(ref)) return null
    const response = await fetch(ref, { redirect: `error` })
    if (!response.ok) return null
    const bytes = new Uint8Array(await response.arrayBuffer())
    if (bytes.byteLength > IMPORT_MAX_ASSET_BYTES) return null
    return {
      bytes,
      contentType: response.headers.get(`content-type`) ?? `application/octet-stream`,
      filename: null,
    }
  },
}

const SOURCES: Record<string, ImportSource> = {
  linear: linearImportSource,
  bundle: bundleImportSource,
}

export function getImportSource(id: string): ImportSource {
  const source = SOURCES[id]
  if (!source) throw new Error(`Unknown import source "${id}"`)
  return source
}

export function isImportSource(id: string): boolean {
  return Object.prototype.hasOwnProperty.call(SOURCES, id)
}
