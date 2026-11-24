// #NEW-134: semantic issue search's server half: one embedding per issue,
// computed in this process (no third party), stored with pgvector.
//
// For #NEW-135 (the kNN branch of `issues.search`):
//
//   if (await issueEmbeddingsAvailable()) {
//     const vector = await embedQuery(query)          // null = stay lexical
//     if (vector) hits = await nearestIssueIds(vector, scope, { k, maxDistance })
//   }
//
// `scope` is `issueSearchMatchIds`' IssueSearchScope: the caller checks
// membership / board access exactly as it does for the lexical branches.
export { embedQuery, embedderConfigured, embedderStats, issueEmbeddingsDisabled } from "./embedder"
export { ISSUE_EMBEDDING_DIMENSIONS, ISSUE_EMBEDDING_MODEL } from "./input"
export {
  ISSUE_EMBEDDING_DEFAULT_K,
  ISSUE_EMBEDDING_MAX_DISTANCE,
  issueEmbeddingsAvailable,
  nearestIssueIds,
  nearestIssuesSql,
  type NearestIssueOptions,
} from "./store"
export {
  recomputeIssueEmbedding,
  runIssueEmbeddingSweep,
  scheduleIssueEmbedding,
  startIssueEmbeddingScheduler,
} from "./recompute"
