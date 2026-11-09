/** #VAPP-182: is this the RFC 8707 `resource` of the apps endpoint
 * (`/api/mcp/apps`)? An MCP client names it when it connects there (its
 * protected-resource metadata says so); the provider keeps it on the signed
 * consent query, so the consent page asks for apps instead of teams and
 * boards. Pure: the consent page (browser) and the server share it. */
export function isAppsResource(resource: string | null | undefined): boolean {
  if (!resource) return false
  try {
    return new URL(resource).pathname.replace(/\/+$/, ``) === `/api/mcp/apps`
  } catch {
    return false
  }
}
