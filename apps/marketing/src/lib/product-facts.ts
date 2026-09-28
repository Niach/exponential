/* Product numbers the marketing copy quotes in more than one place.

   90 = every tool the MCP server can register (88 registerTool calls in
   apps/web/src/lib/mcp/tools.ts, one of them a loop over workflows
   start/pause/cancel). A client sees fewer: the helpdesk and in-run session
   tools register only where they apply. Recount when tools.ts changes. */
export const MCP_TOOL_COUNT = 90
