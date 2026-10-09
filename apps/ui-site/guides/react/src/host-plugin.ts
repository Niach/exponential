// A host plugin = what your app lends a surface: functions it may call,
// sources its data model may follow, and the policy that gates both.
import { ExponentialHost, decideUrl, parseSource } from "@exponential-at/ui"
import type { HostFunction, HostPolicy, SourceResolvers, Transport } from "@exponential-at/ui"

// 1. Functions: `on: {press: {functionCall: {call: "cart.add", args: {sku: "A1"}}}}`.
const functions: Record<string, HostFunction> = {
  "app.toast": ({ text }) => console.info(String(text)),
  "cart.add": async ({ sku }) => (await fetch(`/api/cart`, { method: `POST`, body: JSON.stringify({ sku }) })).ok,
}

// 2. Bindings: `bindDataModel {path: "/weather", source: "weather:current?city=Vienna"}`.
//    The host parses the URI (parseSource) and calls the scheme's resolver.
const sources: SourceResolvers = {
  weather: ({ name, params }, emit) => {
    const poll = async () => emit(await (await fetch(`/api/weather/${name}?city=${encodeURIComponent(params.city ?? ``)}`)).json())
    void poll()
    const timer = setInterval(poll, 60_000)
    return () => clearInterval(timer)
  },
}

// 3. Policy: deny wins, then allow, then ask (your consent UI), then default.
const policy: HostPolicy = {
  functions: { allow: [`app.*`], ask: [`cart.*`], default: `deny` },
  onFunctionCall: (call) => window.confirm(`Allow ${call.name}(${JSON.stringify(call.args)})?`),
  urls: { schemes: [`https`, `mailto`], hosts: [`example.com`, `*.example.com`] },
  openUrl: (url) => window.open(url, `_blank`, `noopener`),
  media: { baseUrl: location.origin, rules: [{ prefix: `${location.origin}/api/`, headers: { authorization: `Bearer ${sessionStorage.token ?? ``}` } }] },
}

export function createHost(transport: Transport): ExponentialHost {
  return new ExponentialHost({ transport, functions, sources, policy })
}

// The same pure rules the host applies, callable on their own:
export const parsed = parseSource(`weather:current?city=Vienna`) // {scheme: "weather", name: "current", params: {city: "Vienna"}}
export const blocked = decideUrl(policy.urls, `https://elsewhere.test/`) // {allowed: false, reason: "host"}
