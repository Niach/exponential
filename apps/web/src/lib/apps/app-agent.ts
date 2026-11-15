import { eq } from "drizzle-orm"
import { agents, apps, type App } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1730 (one-tree model): every app has an AGENT (`apps.agent_id`): the
// app's name, icon and color, owned by its creator. Its root sessions are
// the app's editor chats (#EXP-1849: one per member), its triggers the app's
// automations (#EXP-1848). #EXP-1849: its `device_id` is the app's AGENT
// SLOT: empty (``) = each member's own device (the default), else a device
// shared with the team that runs every member's chat. Created with the app,
// in the same transaction; a standalone (exported) app has none.
export async function createAppAgentInTx(tx: Tx, app: App, userId: string): Promise<string> {
  const deviceId = ``
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
