import { createTRPCProxyClient, httpBatchLink, TRPCClientError } from "@trpc/client"
import type { TRPCLink } from "@trpc/client"
import { observable } from "@trpc/server/observable"
import { toast } from "sonner"
import type { AppRouter } from "@/routes/api/trpc/$"

// Short human nouns per router, used to build "Couldn't update the issue"-style
// messages. Routers not listed fall back to a generic message.
const ROUTER_NOUNS: Record<string, string> = {
  issues: `the issue`,
  comments: `the comment`,
  projects: `the project`,
  labels: `the label`,
  issueLabels: `the issue's labels`,
  workspaces: `the workspace`,
  workspaceMembers: `the member`,
  workspaceInvites: `the invite`,
  notifications: `notifications`,
  subscriptions: `the subscription`,
}

const METHOD_VERBS: Record<string, string> = {
  create: `create`,
  update: `update`,
  delete: `delete`,
  remove: `update`,
  add: `update`,
  revoke: `revoke`,
}

function mutationErrorTitle(path: string): string {
  const [routerName = ``, method = ``] = path.split(`.`)
  const noun = ROUTER_NOUNS[routerName]
  const verb = METHOD_VERBS[method]
  if (noun && verb) return `Couldn't ${verb} ${noun}`
  return `Something went wrong`
}

// Surface the server's TRPCError message when it reads like a sentence (the
// authz layer writes human messages such as "Not a member of this workspace").
// Zod validation errors serialize as a JSON array — skip those.
function serverErrorDetail(error: unknown): string | undefined {
  if (!(error instanceof TRPCClientError)) return undefined
  const message = error.message?.trim()
  if (!message || message.length > 120) return undefined
  if (message.startsWith(`[`) || message.startsWith(`{`)) return undefined
  return message
}

// Central error surfacing for ALL tRPC mutations. Electric-synced writes feel
// instant, so success needs no global feedback — but a silently failed
// mutation looks like data loss. Every mutate() call funnels through this
// link, so individual call sites don't need their own try/catch + toast.
// Opt out per call with `mutate(input, { context: { skipErrorToast: true } })`
// when a site handles the failure itself (e.g. the invite-limit upgrade flow).
const mutationErrorToastLink: TRPCLink<AppRouter> = () => {
  return ({ next, op }) =>
    observable((observer) =>
      next(op).subscribe({
        next: (value) => observer.next(value),
        error: (error) => {
          if (op.type === `mutation` && op.context?.skipErrorToast !== true) {
            toast.error(mutationErrorTitle(op.path), {
              description: serverErrorDetail(error),
            })
          }
          observer.error(error)
        },
        complete: () => observer.complete(),
      })
    )
}

export const trpc = createTRPCProxyClient<AppRouter>({
  links: [
    mutationErrorToastLink,
    httpBatchLink({
      url: `/api/trpc`,
      async headers() {
        return {
          cookie: typeof document !== `undefined` ? document.cookie : ``,
        }
      },
    }),
  ],
})
