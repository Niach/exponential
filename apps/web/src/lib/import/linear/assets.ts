// EXP-630: Linear's uploads are auth-gated (a bare GET is a 401), and its
// docs say to download and self-host — the SAME raw-key `Authorization`
// header the GraphQL API takes works on `uploads.linear.app`. Verified with
// a HEAD against the Methode 5 workspace on 2026-09-23.
import type { FetchedAsset } from "@/lib/import/apply"
import { readBodyBounded, type BoundedBodyResponse } from "@/lib/import/asset-body"
import { IMPORT_ASSET_FETCH_TIMEOUT_MS, IMPORT_MAX_ASSET_BYTES } from "@/lib/import/limits"

export const LINEAR_UPLOAD_HOST = `uploads.linear.app`

export interface AssetFetchLike {
  (
    url: string,
    init: { method: string; headers: Record<string, string>; signal?: AbortSignal }
  ): Promise<{ ok: boolean; status: number } & BoundedBodyResponse>
}

function isUploadUrl(ref: string): boolean {
  try {
    const url = new URL(ref)
    return url.protocol === `https:` && url.hostname === LINEAR_UPLOAD_HOST
  } catch {
    return false
  }
}

export async function probeLinearAssetSize(
  apiKey: string,
  ref: string,
  fetchImpl: AssetFetchLike = globalThis.fetch as unknown as AssetFetchLike
): Promise<number | null> {
  if (!isUploadUrl(ref)) return null
  const response = await fetchImpl(ref, {
    method: `HEAD`,
    headers: { authorization: apiKey },
    signal: AbortSignal.timeout(IMPORT_ASSET_FETCH_TIMEOUT_MS),
  })
  if (!response.ok) return null
  const length = Number(response.headers.get(`content-length`))
  return Number.isFinite(length) && length >= 0 ? length : null
}

// The body streams under a running byte count (asset-body.ts): a wrong or
// missing content-length never buffers past IMPORT_MAX_ASSET_BYTES. The
// timeout signal covers connect AND body; a stalled download throws, and
// the applier's retry (fetchAssetResilient) decides what that costs.
export async function fetchLinearAsset(
  apiKey: string,
  ref: string,
  fetchImpl: AssetFetchLike = globalThis.fetch as unknown as AssetFetchLike
): Promise<FetchedAsset | null> {
  if (!isUploadUrl(ref)) return null
  const response = await fetchImpl(ref, {
    method: `GET`,
    headers: { authorization: apiKey },
    signal: AbortSignal.timeout(IMPORT_ASSET_FETCH_TIMEOUT_MS),
  })
  if (!response.ok) return null
  const bytes = await readBodyBounded(response, IMPORT_MAX_ASSET_BYTES)
  if (!bytes) return null
  return {
    bytes,
    contentType: response.headers.get(`content-type`) ?? `application/octet-stream`,
    filename: null,
  }
}
