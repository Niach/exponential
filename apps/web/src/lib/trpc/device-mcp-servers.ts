// EXP-891: per-device MCP servers (`device_mcp_servers`, server-only, never
// a shape). The DEVICE is the writer: `sync` replaces one machine's whole
// set (what its desktop / CLI holds after an import, an add, a toggle or a
// forget) and authorizes on the caller OWNING the device row, like the
// readiness report. Members READ them per team: every row of every device
// the caller can see in that team (their own machines and the servers
// teammates shared with it — `visibleDeviceRows`), labelled with the device
// and its owner so the web groups them per machine. Config only: a url or a
// command + args, never a header value, a token or an env value.
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray } from "drizzle-orm"
import { mcpTransportSchema } from "@exp/db-schema/domain"
import { router, authedProcedure, type Context } from "@/lib/trpc"
import { deviceMcpServers, devices } from "@/db/schema"
import { assertTeamMember } from "@/lib/team-membership"
import { visibleDeviceRows } from "@/lib/trpc/devices"
import {
  configKey,
  deviceMcpServerAgents,
  deviceMcpServerSources,
  MAX_DEVICE_MCP_SERVER_NAME,
  MAX_DEVICE_MCP_SERVERS,
  validateDeviceMcpServerInput,
  type DeviceMcpServerAgent,
  type DeviceMcpServerListRow,
  type DeviceMcpServerSource,
} from "@/lib/mcp/device-mcp-servers"

const deviceIdInput = z.string().min(1).max(128)

const serverInputSchema = z.object({
  name: z.string().trim().min(1).max(MAX_DEVICE_MCP_SERVER_NAME),
  transport: mcpTransportSchema,
  url: z.string().trim().max(2048).nullish(),
  command: z.string().trim().max(1024).nullish(),
  args: z.array(z.string().max(1024)).max(64).optional(),
  source: z.enum(deviceMcpServerSources),
  agent: z.enum(deviceMcpServerAgents).nullish(),
  enabled: z.boolean(),
})

async function loadOwnDevice(
  db: Context[`db`],
  userId: string,
  deviceId: string
) {
  const [row] = await db
    .select({ id: devices.id, deviceId: devices.deviceId })
    .from(devices)
    .where(and(eq(devices.userId, userId), eq(devices.deviceId, deviceId)))
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
  }
  return row
}

export const deviceMcpServersRouter = router({
  // Every device row visible to the caller in `teamId`, captioned for the
  // web's per-machine groups.
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }): Promise<DeviceMcpServerListRow[]> => {
      const userId = ctx.session.user.id
      await assertTeamMember(userId, input.teamId)
      const visible = await visibleDeviceRows(ctx.db, userId, input.teamId)
      if (visible.rows.length === 0) return []
      const deviceById = new Map(visible.rows.map((row) => [row.id, row]))
      const rows = await ctx.db
        .select()
        .from(deviceMcpServers)
        .where(inArray(deviceMcpServers.deviceRowId, [...deviceById.keys()]))
        .orderBy(asc(deviceMcpServers.name))
      const myName = ctx.session.user.name ?? ``
      return rows.flatMap((row) => {
        const device = deviceById.get(row.deviceRowId)
        if (!device) return []
        const owner = visible.ownerNames.get(device.userId)
        return [
          {
            id: row.id,
            deviceRowId: row.deviceRowId,
            deviceId: device.deviceId,
            deviceLabel: device.label,
            userId: row.userId,
            ownerName: device.userId === userId ? myName : (owner?.name ?? ``),
            name: row.name,
            transport: row.transport as DeviceMcpServerListRow[`transport`],
            url: row.url,
            command: row.command,
            args: row.args,
            source: row.source as DeviceMcpServerSource,
            agent: (row.agent as DeviceMcpServerAgent | null) ?? null,
            enabled: row.enabled,
            updatedAt: row.updatedAt.toISOString(),
          },
        ]
      })
    }),

  // The device's whole set, replacing what the server held for it: rows
  // are upserted by name, and names it no longer lists are deleted. An
  // empty list clears the machine. Refuses (BAD_REQUEST, naming the row)
  // before writing anything, so a bad row never half-applies.
  sync: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        servers: z.array(serverInputSchema).max(MAX_DEVICE_MCP_SERVERS),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const device = await loadOwnDevice(ctx.db, userId, input.deviceId)
      const keys = new Map<string, string>()
      const names = new Set<string>()
      for (const server of input.servers) {
        const problem = validateDeviceMcpServerInput(server)
        if (problem) throw new TRPCError({ code: `BAD_REQUEST`, message: problem })
        const name = server.name.trim()
        if (names.has(name)) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `${name} is listed twice`,
          })
        }
        names.add(name)
        const key = configKey(name)
        const clash = keys.get(key)
        if (clash) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `${name} and ${clash} would share the config key ${key}`,
          })
        }
        keys.set(key, name)
      }
      const now = new Date()
      await ctx.db.transaction(async (tx) => {
        const held = await tx
          .select({ name: deviceMcpServers.name })
          .from(deviceMcpServers)
          .where(eq(deviceMcpServers.deviceRowId, device.id))
        const stale = held
          .map((row) => row.name)
          .filter((name) => !names.has(name))
        if (stale.length > 0) {
          await tx
            .delete(deviceMcpServers)
            .where(
              and(
                eq(deviceMcpServers.deviceRowId, device.id),
                inArray(deviceMcpServers.name, stale)
              )
            )
        }
        for (const server of input.servers) {
          const http = server.transport === `http`
          const values = {
            deviceId: device.deviceId,
            userId,
            transport: server.transport,
            url: http ? server.url!.trim() : null,
            command: http ? null : server.command!.trim(),
            args: http ? [] : (server.args ?? []),
            source: server.source,
            agent: server.agent ?? null,
            enabled: server.enabled,
            updatedAt: now,
          }
          await tx
            .insert(deviceMcpServers)
            .values({ ...values, deviceRowId: device.id, name: server.name.trim() })
            .onConflictDoUpdate({
              target: [deviceMcpServers.deviceRowId, deviceMcpServers.name],
              set: values,
            })
        }
      })
      return { ok: true as const, count: names.size }
    }),
})
