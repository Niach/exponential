// The transaction handle every session-tree writer takes (drizzle's tx).
export type Tx = Parameters<
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]
// eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
export type Db = typeof import("@/db/connection").db | Tx
