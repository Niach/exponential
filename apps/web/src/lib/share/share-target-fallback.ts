// EXP-1635: the manifest's `share_target` POST with no service worker in
// front of it (the worker owns it otherwise, `public/sw.js`). Text survives
// through `/share`'s search params; files do not, and a body too big to hold
// a text share goes unread.

const MAX_TEXT_BODY_BYTES = 256 * 1024
/** Each field's room in the redirect URL. */
const MAX_FIELD_CHARS = 2_000

async function shareFields(request: Request): Promise<URLSearchParams> {
  const params = new URLSearchParams()
  const length = Number(request.headers.get(`content-length`) ?? Number.NaN)
  if (!(length <= MAX_TEXT_BODY_BYTES)) return params
  const form = await request.formData().catch(() => null)
  for (const name of [`title`, `text`, `url`]) {
    const value = form?.get(name)
    if (typeof value === `string` && value) params.set(name, value.slice(0, MAX_FIELD_CHARS))
  }
  return params
}

export async function shareTargetFallback(request: Request): Promise<Response> {
  const query = (await shareFields(request)).toString()
  return new Response(null, {
    status: 303,
    headers: { location: query ? `/share?${query}` : `/share` },
  })
}
