/**
 * Dev only (#EXP-1990): makes a team agent ASK in its group chat, the way a
 * turn of its device would, so the question's push can be checked by hand
 * without a running agent. It consumes the chat's pending input (the
 * device's `consume` once a turn has begun), then posts ONE question card
 * as the chat's own session through the server path `exponential_show
 * {ask}` takes (`postAskCard`); sealed with the tree's PUBLIC key when the
 * chat has one, so the apps that hold its secret open it. The push goes to
 * the asker (agent-settings.json `groupChat.askCases`).
 *
 * Usage (from apps/web, against a LOCAL dev database only):
 *   bun scripts/seed-group-chat-ask.ts <agentId> ["question?"]
 */
import { and, eq, isNull, sql } from "drizzle-orm"
import { sealCard } from "@exp/domain-contract/sealed-content"
import { db } from "@/db/connection"
import { agents, messages, sessions } from "@/db/schema"
import { postAskCard } from "@/lib/sessions/plan"
import { inSession, pendingInput } from "@/lib/sessions/messages"
import { assertLocalDatabase } from "./lib/local-db-guard"

async function main() {
  assertLocalDatabase(`this posts a question card into a team agent's group chat.`)
  const [agentId, question = `Ship it today, or wait for the review?`] = process.argv.slice(2)
  if (!agentId) throw new Error(`usage: seed-group-chat-ask.ts <agentId> ["question?"]`)
  const [agent] = await db.select().from(agents).where(eq(agents.id, agentId)).limit(1)
  if (!agent?.groupChatId) throw new Error(`agent ${agentId} has no group chat yet (is AGENT_GROUP_CHATS on, and did someone write to it?)`)
  const [chat] = await db.select().from(sessions).where(eq(sessions.id, agent.groupChatId)).limit(1)
  if (!chat) throw new Error(`the group chat ${agent.groupChatId} is gone`)
  const now = new Date()
  const consumed = await db
    .update(messages)
    .set({ consumedAt: now, deliveredAt: sql`COALESCE(${messages.deliveredAt}, ${now})`, updatedAt: now })
    .where(and(inSession(chat.id), pendingInput(), isNull(messages.consumedAt)))
    .returning({ id: messages.id })
  const clear = { kind: `question` as const, question, options: [`Ship it`, `Wait`] }
  const card = chat.sealPublicKey && chat.sealEpoch ? sealCard(chat.sealPublicKey, chat.sealEpoch, clear) : clear
  const { messageId } = await postAskCard({ userId: chat.userId, sessionId: chat.id, card: card as never })
  console.log(`consumed\t${consumed.length}\nquestion\t${messageId}\nchat\t${chat.id}`)
  // The push is fire-and-forget: let it land before the process ends.
  await new Promise((resolve) => setTimeout(resolve, 1500))
  process.exit(0)
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
