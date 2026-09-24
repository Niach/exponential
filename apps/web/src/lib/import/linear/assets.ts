// EXP-630: Linear's uploads are auth-gated (a bare GET is a 401), and its
// docs say to download and self-host — the SAME raw-key `Authorization`
// header the GraphQL API takes works on `uploads.linear.app`. Verified with
// a HEAD against the Methode 5 workspace on 2026-09-23.
import type { FetchedAsset } from "@/lib/import/apply"
import { IMPORT_MAX_ASSET_BYTES } from "@/lib/import/limits"

export const LINEAR_UPLOAD_HOST = `uploads.linear.app`

export interface AssetFetchLike {
  (
    url: string,
    init: { method: string; headers: Record<string, string> }
  ): Promise<{
    ok: boolean
    status: number
    headers: { get(name: string): string | null }
    arrayBuffer(): Promise<ArrayBuffer>
  }>
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
  })
  if (!response.ok) return null
  const length = Number(response.headers.get(`content-length`))
  return Number.isFinite(length) && length >= 0 ? length : null
}

export async function fetchLinearAsset(
  apiKey: string,
  ref: string,
  fetchImpl: AssetFetchLike = globalThis.fetch as unknown as AssetFetchLike
): Promise<FetchedAsset | null> {
  if (!isUploadUrl(ref)) return null
  const response = await fetchImpl(ref, {
    method: `GET`,
    headers: { authorization: apiKey },
  })
  if (!response.ok) return null
  const declared = Number(response.headers.get(`content-length`))
  if (Number.isFinite(declared) && declared > IMPORT_MAX_ASSET_BYTES) return null
  const bytes = new Uint8Array(await response.arrayBuffer())
  if (bytes.byteLength > IMPORT_MAX_ASSET_BYTES) return null
  return {
    bytes,
    contentType: response.headers.get(`content-type`) ?? `application/octet-stream`,
    filename: null,
  }
}
