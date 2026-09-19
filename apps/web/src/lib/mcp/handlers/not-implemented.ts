// EXP-988: the workflow contract's stub marker. Every handler under
// `lib/mcp/handlers/` starts as a signature that throws this; the leaf that
// owns the file replaces the throw with the behaviour. The registry
// (`lib/mcp/tools.ts`) is NOT edited by a leaf — it already does the access
// checks and calls into here.
export class NotImplementedError extends Error {
  constructor(what: string, owner: string) {
    super(`${what} is not implemented yet (${owner})`)
    this.name = `NotImplementedError`
  }
}
