// EXP-1183 — the MCP Apps views as components, for the styleguide's islands.
// The app itself is the built `app.html` (vite.config.ts).
export { IssueDetailView } from "./issue-detail-view"
export { IssueListView } from "./issue-list-view"
export { RunView } from "./run-view"
export {
  MCP_APP_VIEWS,
  MCP_APP_VIEW_TOOL,
  type IssueDetail,
  type IssueRow,
  type McpAppView,
  type RunDetail,
} from "./model"
