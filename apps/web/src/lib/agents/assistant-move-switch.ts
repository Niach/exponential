// #EXP-1986: THE switch of the Assistant move (`ASSISTANT_MOVE=on`), alone
// in a module without the database so the client-version gate can read it.

/** Whether the move is switched on (read per call, so tests can flip it). */
export function assistantMoveOn(): boolean {
  return (process.env.ASSISTANT_MOVE ?? ``).trim().toLowerCase() === `on`
}
