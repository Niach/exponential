// EXP-353/EXP-637/EXP-1051: what THIS deployment's Exponential MCP surface
// costs an agent's context window on turn one.
//
// `serializeToolDefs` mirrors the MCP SDK's `tools/list` serialization — it is
// what `context-budget.test.ts` measures the ceilings against, and (EXP-1051)
// what `codingSessions.contextBudget` hands the launcher so a run's context
// bar can attribute its `tools` layer to a real number rather than a guess.
//
// Deliberately a SERVER-side constant: the tool set is whatever this build
// registers (the cloud-only `exponential_report_bug` included or not), so it
// is a deploy fact, not a per-run one.

import { z } from "zod"
import { registerExponentialTools } from "@/lib/mcp/tools"
import { FULL_ACCESS } from "@/lib/mcp/scope"
import { ALL_MCP_TOOL_GATES } from "@/lib/mcp/gates"
import {
  ALWAYS_LOAD_TOOLS,
  GATED_ALWAYS_LOAD_TOOLS,
} from "@/lib/mcp/always-load"
import { MCP_SERVER_INSTRUCTIONS } from "@/lib/mcp/instructions"
import type { McpUser } from "@/lib/mcp/server"

type ToolDef = {
  description?: string
  // EXP-705: every tool passes a strict z.object INSTANCE, not a raw shape.
  inputSchema?: z.ZodType
  _meta?: Record<string, unknown>
}

export function serializeToolDefs(gates = ALL_MCP_TOOL_GATES) {
  const defs: Array<Record<string, unknown>> = []
  const fakeServer = {
    registerTool: (name: string, def: ToolDef) => {
      // Mirror the MCP SDK's tools/list serialization (name + description +
      // JSON-schema'd input; draft-7 target like zod-json-schema-compat).
      defs.push({
        name,
        description: def.description,
        inputSchema: z.toJSONSchema(def.inputSchema ?? z.strictObject({}), {
          io: `input`,
          target: `draft-7`,
        }),
        ...(def._meta ? { _meta: def._meta } : {}),
      })
    },
  }
  registerExponentialTools(
    fakeServer as never,
    { id: `u` } as unknown as McpUser,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS,
    null,
    gates
  )
  return defs
}

/** EXP-1051: the two UTF-8 byte counts an Exponential MCP server adds to
 *  every coding run's first turn. */
export interface McpContextBudget {
  /** The always-loaded tool definitions, serialized exactly as a client
   *  receives them (`ALWAYS_LOAD_TOOLS` + `GATED_ALWAYS_LOAD_TOOLS`, the
   *  gated one because an unattended run carries it from turn one). */
  mcpAlwaysLoadBytes: number
  /** The server `instructions` string — the only guidance a deferred-tool
   *  client sees up front. */
  mcpInstructionsBytes: number
}

let cached: McpContextBudget | null = null

/** What this deployment's MCP surface weighs, in UTF-8 bytes. Memoized per
 *  process: the registration is pure (it walks the same tool table every
 *  time) and nothing about it varies per caller.
 *
 *  An ESTIMATE, and deliberately a low one: the DEFERRED tools' NAMES also sit
 *  in the window (a client lists them so tool search can find them) and are
 *  not counted here — only the definitions that load verbatim plus the
 *  instructions. The consumer (the device's `context_layout` `tools` segment)
 *  labels the number `estimated` for exactly that reason. */
export function mcpContextBudget(): McpContextBudget {
  if (cached) return cached
  const alwaysLoadedNames: readonly string[] = [
    ...ALWAYS_LOAD_TOOLS,
    ...GATED_ALWAYS_LOAD_TOOLS,
  ]
  const alwaysLoaded = serializeToolDefs().filter((def) =>
    alwaysLoadedNames.includes(def.name as string)
  )
  cached = {
    mcpAlwaysLoadBytes: Buffer.byteLength(JSON.stringify(alwaysLoaded)),
    mcpInstructionsBytes: Buffer.byteLength(MCP_SERVER_INSTRUCTIONS),
  }
  return cached
}
