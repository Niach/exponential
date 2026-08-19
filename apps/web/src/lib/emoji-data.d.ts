// EXP-551 — type for the generated emoji dataset. apps/web/tsconfig.json has
// no `resolveJsonModule` (TS would otherwise read the 245KB file on every
// typecheck), so the module is declared ambiently and Vite bundles it into a
// lazy chunk at runtime (`lib/emoji.ts` dynamic-imports it).
declare module "@/lib/emoji.generated.json" {
  import type { EmojiDataset } from "@exp/emoji"
  const data: EmojiDataset
  export default data
}
