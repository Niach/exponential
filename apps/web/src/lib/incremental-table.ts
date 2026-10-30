// VAPP-159: a synced collection kept as a mapped table INCREMENTALLY: each
// change maps only its own rows (insert/update = map, delete or a row the
// map drops = removed), so a binding over thousands of rows no longer
// re-reads and re-maps the whole collection on every change. The array the
// derivations read is rebuilt lazily, once per change batch.

export interface TableChange<T> {
  type: `insert` | `update` | `delete`
  key: string | number
  value: T
}

export interface ChangeSource<T> {
  subscribeChanges: (
    callback: (changes: TableChange<T>[]) => void,
    options?: { includeInitialState?: boolean; where?: (row: never) => unknown }
  ) => { unsubscribe: () => void }
}

export interface IncrementalTable<R> {
  /** The mapped rows (one stable array between changes). */
  rows: () => readonly R[]
  get: (key: string | number) => R | undefined
  /** Bumped on every applied batch: memo key for derived views. */
  version: () => number
  unsubscribe: () => void
}

/** Subscribes to `source` (initial state included) and keeps `map(row)` per
 *  key; `map` returning null keeps the row out (a filter). `onChange` runs
 *  after each applied batch. `where` narrows the subscription itself (an
 *  on-demand collection loads exactly that subset). */
export function incrementalTable<T, R>(
  source: ChangeSource<T>,
  map: (row: T) => R | null,
  onChange: () => void = () => {},
  where?: (row: never) => unknown
): IncrementalTable<R> {
  const byKey = new Map<string | number, R>()
  let cache: R[] | null = null
  let version = 0
  const sub = source.subscribeChanges(
    (changes) => {
      for (const change of changes) {
        const mapped = change.type === `delete` ? null : map(change.value)
        if (mapped === null) byKey.delete(change.key)
        else byKey.set(change.key, mapped)
      }
      cache = null
      version++
      onChange()
    },
    where ? { includeInitialState: true, where } : { includeInitialState: true }
  )
  return {
    rows: () => (cache ??= [...byKey.values()]),
    get: (key) => byKey.get(key),
    version: () => version,
    unsubscribe: () => sub.unsubscribe(),
  }
}

/** A derived value recomputed only when one of its inputs' versions moved. */
export function memoByVersions<V>(versions: () => readonly number[], compute: () => V): () => V {
  let last: readonly number[] | null = null
  let value: V
  return () => {
    const now = versions()
    if (last && now.length === last.length && now.every((v, i) => v === last![i])) return value
    last = now
    value = compute()
    return value
  }
}
