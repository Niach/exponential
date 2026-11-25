// #EXP-2158: run in the web image (build-web.yml's smoke). Bun 1.3.14 kept
// every body stream that was opened and not read to its end, with the whole
// request context (an aborted tRPC call, a HEAD request, a diagnostics
// envelope): ~2,000 an hour on production. Fails when the runtime does again.
import { heapStats } from "bun:jsc"

const ROUNDS = 300
const server = Bun.serve({
  port: 0,
  async fetch(request) {
    const reader = request.body?.getReader()
    while (reader && !(await reader.read()).done);
    return new Response(null, { status: 413 })
  },
})

for (let i = 0; i < ROUNDS; i++) {
  Response.json({ i }).body.getReader()
  Response.json({ i }).body.pipeThrough(new TransformStream())
  await (await fetch(server.url, { method: `POST`, body: `x`.repeat(3000) })).text()
}
for (let i = 0; i < 3; i++) {
  Bun.gc(true)
  await Bun.sleep(200)
}
const types = heapStats().objectTypeCounts
const left = { streams: types.ReadableStream ?? 0, sources: types.BlobInternalReadableStreamSource ?? 0 }
console.log(`bun ${Bun.version}: ${JSON.stringify(left)} left of ${ROUNDS * 3}`)
process.exit(left.streams + left.sources > ROUNDS / 10 ? 1 : 0)
