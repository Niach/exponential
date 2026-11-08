import { eq } from "drizzle-orm"
import { agents, apps, type App } from "@/db/schema"
import { defaultDeviceFor } from "@/lib/apps/editor-session"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1730 (one-tree model): every app has an AGENT (`apps.agent_id`): the
// app's name, icon and color, owned by its creator, on their default device
// (the agent's `defaults`). Its root session is the app's editor; its
// triggers carry the app's automations (phase B). Created with the app, in
// the same transaction; a standalone (exported) app has none.
export async function createAppAgentInTx(tx: Tx, app: App, userId: string): Promise<string> {
  const deviceId = (await defaultDeviceFor(userId)) ?? ``
  const [agent] = await tx
    .insert(agents)
    .values({
      teamId: app.teamId,
      name: app.name,
      icon: app.icon,
      color: app.color,
      deviceId,
      ownerId: userId,
      role: `agent`,
    })
    .returning({ id: agents.id })
  await tx.update(apps).set({ agentId: agent!.id }).where(eq(apps.id, app.id))
  return agent!.id
}
