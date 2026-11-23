// #EXP-1990: THE switch of team agents' group chats (`AGENT_GROUP_CHATS=on`),
// alone in a module without the database so the client-version gate can
// read it.

/** Whether team agents talk in ONE group chat (read per call, so tests can flip it). */
export function agentGroupChatsOn(): boolean {
  return (process.env.AGENT_GROUP_CHATS ?? ``).trim().toLowerCase() === `on`
}
