// EXP-1459: THE web path for a person's `sessions.send` and
// `sessions.answer`: seal-send.ts over this tab's KeyHolder and the synced
// collections, then the tRPC call. Every composer, menu line and surface
// reply sends through `sendSessionMessage`; every card answer through
// `answerSessionCard`.

import type { SessionsSendInput } from "@exp/db-schema/domain"
import { deviceCollection, messageCollection, sessionCollection, userCollection } from "@/lib/collections"
import type { CardAnswerInput } from "@/lib/sessions/card-answer"
import { trpc } from "@/lib/trpc-client"
import { currentKeyHolder } from "./current-key-holder"
import { sealAnswerInput, sealSendInput, type SealDeps, type SealSessionRow } from "./seal-send"

type TrpcOptions = Parameters<typeof trpc.sessions.send.mutate>[1]

const quiet = { context: { skipErrorToast: true } }

function sealDeps(): SealDeps {
  const holder = currentKeyHolder()
  return {
    holder,
    // The holder belongs to the signed-in account (no holder = no sealing).
    me: holder?.userId ?? ``,
    session: (id) => sessionCollection.get(id) as SealSessionRow | undefined,
    sessions: () => [...sessionCollection.values()] as SealSessionRow[],
    deviceRowIds: (machineId) =>
      [...deviceCollection.values()].filter((row) => row.deviceId === machineId).map((row) => row.id),
    myInstalls: () => trpc.installs.mine.query(undefined, quiet),
    teamInstalls: (teamId) => (teamId ? trpc.installs.list.query({ teamId }, quiet) : Promise.resolve([])),
    userIdsByEmail: (emails) => {
      const wanted = new Set(emails)
      return [...userCollection.values()]
        .filter((row) => typeof row.email === `string` && wanted.has(row.email.toLowerCase()))
        .map((row) => row.id)
    },
  }
}

/** `sessions.send`, sealed when this browser holds keys. */
export async function sendSessionMessage(input: SessionsSendInput, options?: TrpcOptions) {
  const sealed = await sealSendInput(input, sealDeps())
  const result = await trpc.sessions.send.mutate(sealed.input as SessionsSendInput, options)
  await sealed.after(result).catch(() => {})
  return result
}

/** `sessions.answer`; a sealed card's answer is re-sealed inside it. */
export async function answerSessionCard(messageId: string, input: CardAnswerInput, options?: TrpcOptions) {
  const row = messageCollection.get(messageId) as { card: Record<string, unknown> | null; rootId: string } | undefined
  const root = row ? (sessionCollection.get(row.rootId) as SealSessionRow | undefined) : undefined
  const payload = row ? sealAnswerInput(row, root, input, currentKeyHolder()) : input
  return trpc.sessions.answer.mutate(
    { messageId, ...payload } as Parameters<typeof trpc.sessions.answer.mutate>[0],
    options as Parameters<typeof trpc.sessions.answer.mutate>[1]
  )
}
