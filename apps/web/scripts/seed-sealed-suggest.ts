/**
 * Dev only (#EXP-1958): posts a SEALED replies card and a SEALED suggestions
 * card on a session of a keyed tree, through the server path a sealing
 * device's `sessions_suggest {sealed}` takes (`postSealedSuggestionCard`).
 * Sealing needs only the tree's PUBLIC key (on the root row), so the app
 * that holds the tree's secret opens them; the server never sees a word.
 *
 * Usage (from apps/web, against a LOCAL dev database only):
 *   bun scripts/seed-sealed-suggest.ts <sessionId> [replies|suggestions|both]
 */
import { eq } from "drizzle-orm"
import { sealCard } from "@exp/domain-contract/sealed-content"
import { db } from "@/db/connection"
import { sessions } from "@/db/schema"
import { postSealedSuggestionCard } from "@/lib/sessions/plan"

async function main() {
  const [sessionId, which = `both`] = process.argv.slice(2)
  if (!sessionId) throw new Error(`usage: seed-sealed-suggest.ts <sessionId> [replies|suggestions|both]`)
  const [session] = await db.select().from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!session) throw new Error(`no session ${sessionId}`)
  const [root] = await db.select().from(sessions).where(eq(sessions.id, session.rootId)).limit(1)
  if (!root?.sealPublicKey || !root.sealEpoch) throw new Error(`the tree of ${sessionId} has no key yet`)
  const seal = (card: Record<string, unknown>) => sealCard(root.sealPublicKey!, root.sealEpoch!, card)
  const cards: Record<string, unknown>[] = []
  if (which !== `suggestions`) {
    cards.push(
      seal({
        kind: `replies`,
        items: [
          { id: crypto.randomUUID(), text: `Yes, ship it` },
          { id: crypto.randomUUID(), text: `Run the tests first` },
        ],
      })
    )
  }
  if (which !== `replies`) {
    cards.push(
      seal({
        kind: `suggestions`,
        items: [
          { id: crypto.randomUUID(), title: `Add tests`, text: `Cover the sealed tap with a real-db test.` },
          { id: crypto.randomUUID(), title: `Update the docs`, text: `Describe the stamps in the guide.` },
        ],
      })
    )
  }
  for (const card of cards) {
    const { messageId } = await postSealedSuggestionCard({ userId: session.userId, callerSessionId: session.id, card })
    console.log(`${card.kind}\t${messageId}`)
  }
  process.exit(0)
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
