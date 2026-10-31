// #VAPP-186: which app's MCP tool a request runs for. The apps broker
// (apps-broker.ts) runs an `exponential.<noun>.<op>` binding in-process on a
// Request it creates itself and tags here; the MCP tools' tRPC context
// (tools.ts buildCtx) reads the tag, so a write attributes "via <app>".
// A WeakMap keyed by the Request OBJECT, never a header: nothing a client
// sends can claim an app.
const viaApp = new WeakMap<Request, string>()

/** A fresh Request (same URL and headers) tagged with `appId`. */
export function requestViaApp(request: Request, appId: string): Request {
  const tagged = new Request(request.url, { headers: request.headers })
  viaApp.set(tagged, appId)
  return tagged
}

/** The app a request runs for, or undefined. */
export function viaAppOf(request: Request): string | undefined {
  return viaApp.get(request)
}
