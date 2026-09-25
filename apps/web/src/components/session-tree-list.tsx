export {
  SessionTree as SessionTreeList,
  type TreeListRow,
} from "@/components/session-tree"

// EXP-996: `SessionTreeList` IS the session tree now. The name stays because
// four call sites read it (the sidebar's automated-runs nav, the Automations
// tab, both Recent lists) and none of them cares that the nesting grew
// workflow and stack groups — the rule moved, not the list.
