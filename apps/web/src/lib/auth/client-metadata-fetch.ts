import { lookup as dnsLookup } from "node:dns/promises"
import { request as httpsRequest } from "node:https"
import { isIP } from "node:net"
import { Readable } from "node:stream"
import { isPublicAddress, type AddressLookup } from "@/lib/import/public-address"

// #NEW-141: the door an MCP client's metadata document (CIMD: its `client_id`
// is the document's URL) is fetched through. ANYONE can name a URL on the
// authorize endpoint, so the transport is the one @better-auth/cimd asks for:
// the host is resolved ONCE, every answer must be public, the connection is
// pinned to the address that was checked (TLS still names and verifies the
// host), redirects are refused. Self-host and cloud alike; a metadata document
// never sits on a private network. The plugin owns the timeout, the 5 KB body
// cap and the fetch budget.

export class ClientMetadataFetchError extends Error {}

const resolveAll: AddressLookup = (host) => dnsLookup(host, { all: true })

/** The one address to connect to; throws when ANY answer is not public. */
export async function pinnedAddress(host: string, lookup: AddressLookup = resolveAll): Promise<string> {
  const literal = host.replace(/^\[|\]$/g, ``)
  const answers = isIP(literal) ? [literal] : (await lookup(literal)).map((answer) => answer.address)
  if (answers.length === 0 || !answers.every(isPublicAddress)) throw new ClientMetadataFetchError(`${host} is not a public address`)
  // A container often has no IPv6 route; a host with both answers on IPv4.
  return answers.find((address) => isIP(address) === 4) ?? answers[0]!
}

type Connect = typeof httpsRequest

export function clientMetadataFetch(lookup: AddressLookup = resolveAll, connect: Connect = httpsRequest) {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = new URL(input instanceof Request ? input.url : input)
    if (url.protocol !== `https:` || url.username || url.password) throw new ClientMetadataFetchError(`only plain https:// URLs`)
    const method = (init?.method ?? `GET`).toUpperCase()
    if (method !== `GET`) throw new ClientMetadataFetchError(`only GET`)
    const address = await pinnedAddress(url.hostname, lookup)
    const literal = isIP(url.hostname.replace(/^\[|\]$/g, ``)) !== 0
    return new Promise<Response>((resolve, reject) => {
      const req = connect(
        {
          host: address,
          ...(literal ? {} : { servername: url.hostname }),
          port: url.port || 443,
          path: `${url.pathname}${url.search}`,
          method,
          headers: { ...Object.fromEntries(new Headers(init?.headers)), host: url.host, "accept-encoding": `identity` },
          agent: false,
          signal: init?.signal ?? undefined,
        },
        (res) => {
          const status = res.statusCode ?? 0
          if (status >= 300 && status < 400 && status !== 304) {
            res.destroy()
            reject(new ClientMetadataFetchError(`answered with a redirect`))
            return
          }
          const headers = new Headers()
          for (const [name, value] of Object.entries(res.headers)) if (value !== undefined) headers.set(name, Array.isArray(value) ? value.join(`, `) : value)
          const bodyless = status === 204 || status === 205 || status === 304
          if (bodyless) res.resume()
          resolve(new Response(bodyless ? null : (Readable.toWeb(res) as unknown as ReadableStream), { status, headers }))
        }
      )
      req.on(`error`, () => reject(new ClientMetadataFetchError(`could not connect`)))
      req.end()
    })
  }
}
