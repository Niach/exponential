import type { SessionsSendInput } from "@exp/db-schema/domain"
import { trpc } from "@/lib/trpc-client"

// #EDE-97: a test double for `lib/enet/send-sealed` — the send as given
// through the (mocked) tRPC client, for suites that render a composer but
// test something else: the real writer never sends in clear without a key
// holder (it waits `clearTree.holderWaitMs`, then fails). Use:
// vi.mock(`@/lib/enet/send-sealed`, () => import(`@/test/send-plain`))

type TrpcOptions = Parameters<typeof trpc.sessions.send.mutate>[1]

export function sendSessionMessage(input: SessionsSendInput, options?: TrpcOptions) {
  return trpc.sessions.send.mutate(input, { ...options, context: { ...options?.context, resealOnStaleTree: true } })
}
