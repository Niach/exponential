import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (fixtures/chats.json): placeholder until its lane fills it.
export const entry: StyleguideEntry = {
  id: `chat-menus`,
  section: `special`,
  owner: `EXP-1373`,
  title: `Chat menus`,
  blurb: `See packages/domain-contract/fixtures/chats.json.`,
  placeholder: true,
  render: () => `<p>Placeholder: EXP-1373 fills this entry.</p>`,
}
