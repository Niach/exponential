// REV2-20: Better Auth's customSession plugin SWALLOWS a failed session
// lookup. Its /get-session endpoint re-invokes the core handler wrapped in
// `.catch(() => null)` (better-auth/plugins/custom-session), and the core
// handler turns any non-API error — a Postgres restart, a killed connection —
// into a thrown INTERNAL_SERVER_ERROR. So the outage arrives at resolveSession
// looking exactly like a logged-out request, and the degradation is
// destructive: shape-route answers 401 to token clients (the desktop sync
// engine deletes its stored token on a SINGLE 401) and silently swaps web
// clients onto the anonymous sentinel shape, rotating the Electric shape
// identity and forcing a full refetch storm.
//
// The swallow lives inside the plugin, so watch the layer BELOW it instead:
// every auth database call goes through the adapter. This wrapper counts
// adapter failures; resolveSession samples the counter around getSession, and
// a null session with a bumped counter is a lookup FAILURE, not a missing one.
//
// The counter is process-global rather than request-scoped, so a failure on
// some other in-flight request can make a genuinely anonymous request answer
// 503 instead of 401/anonymous. That direction is deliberate: a spurious 503
// costs one client retry, a spurious 401 costs a desktop user their credential.

let failureCount = 0

export function authDbFailureCount(): number {
  return failureCount
}

// Called by the adapter wrapper below — exported so tests can simulate the
// swallowed failure without standing up a database.
export function noteAuthDbFailure(): void {
  failureCount++
}

// Wraps a Better Auth adapter factory so every failing adapter call bumps the
// counter. Pass-through in every other respect: the original error is
// rethrown untouched and non-function members are returned as-is.
export function withAuthDbFailureSignal<
  F extends (...args: never[]) => object,
>(adapterFactory: F): F {
  return ((...args: Parameters<F>) =>
    countAdapterFailures(adapterFactory(...args))) as F
}

function countAdapterFailures<A extends object>(adapter: A): A {
  const wrapped = new Map<string | symbol, unknown>()
  return new Proxy(adapter, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver)
      if (typeof value !== `function`) return value
      const cached = wrapped.get(prop)
      if (cached) return cached
      const method = value as (...args: unknown[]) => unknown
      const fn = (...args: unknown[]) => {
        try {
          const result = method.apply(target, args)
          if (result instanceof Promise) {
            return result.catch((err: unknown) => {
              noteAuthDbFailure()
              throw err
            })
          }
          return result
        } catch (err) {
          noteAuthDbFailure()
          throw err
        }
      }
      wrapped.set(prop, fn)
      return fn
    },
  })
}
