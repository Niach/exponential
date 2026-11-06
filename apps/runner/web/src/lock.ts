// VAPP-269: one tab per origin runs the app (runner.json `busy`): the tab that
// holds this Web Lock owns the instance and its stores for its whole life; a
// second tab paints `busy`, waits on the lock, and opens the app when the
// first one closes. Never a second store.

/** The lock one tab of the origin holds while it runs the app (runner.json `busy`). */
export const LOCK = `exp-runner`

/** Hold the origin's runner lock for the page's life; `waiting` when another tab holds it. */
export function holdLock(locks: LockManager | undefined, waiting: () => void): Promise<void> {
  if (!locks) return Promise.resolve()
  return new Promise((resolve) => {
    const forever = () => new Promise<never>(() => {})
    void locks.request(LOCK, { ifAvailable: true }, (lock) => {
      if (lock) {
        resolve()
        return forever()
      }
      waiting()
      void locks.request(LOCK, () => (resolve(), forever()))
      return undefined
    })
  })
}
