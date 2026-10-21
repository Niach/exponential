#!/usr/bin/env bun
// Emits the canonical enum value lists from contract.json into Swift (iOS),
// Kotlin (Android), and Rust (apps/desktop) sources.
// Run from the repo root with: `bun run --filter @exp/domain-contract generate`.
//
// The mobile native enums (IssueStatus.swift, IssueStatus.kt, …) are
// hand-written so they can carry language-specific affordances (SF Symbols,
// Material icons, colors); their raw values must stay in sync with these
// generated constants. On the web side, contract.json parity is enforced by
// the domain-contract drift test (apps/web/src/lib/domain-contract.test.ts).

import { writeFileSync, mkdirSync, readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const repoRoot = join(__dirname, "..", "..", "..")

interface Section {
  values: string[]
  displayOrder?: string[]
}
interface Contract {
  issueStatus: Section
  issueStatusCategory: Section & { startedMax: number }
  issueStatusDefaults: {
    key: string
    category: string
    name: string
    color: string
    sortOrder: number
  }[]
  issuePriority: Section
  issueSource: Section
  issueEstimation: Section
  teamRole: Section
  boardIcon: Section
  deviceIcon: Section
  commentKind: Section
  commentSource: Section
  commentAudience: Section
  notificationType: Section
  prState: Section
  sessionKind: Section
  sessionStatus: Section
  codingSessionBlocked: { kinds: string[]; windows: string[] }
  codingSession: {
    staleHours: number
    launchKeys: string[]
    computerUseCap: string
  }
  device: { onlineWindowSeconds: number }
  team: { agentPromptMaxBytes: number }
  steerFeed: {
    byteCap: number
    itemCap: number
    trimTargetPercent: number
    itemOverheadBytes: number
    window: number
    windowStep: number
    historyPageMax: number
    toolDiffMaxLines: number
    toolDiffMaxBytes: number
    toolOutputMaxLines: number
    toolOutputMaxBytes: number
  }
  composerUi: {
    runHeadline: string
    implementHeadline: string
    chatHeadline: string
    chatPlaceholder: string
    instructionsPlaceholder: string
    dialogTitle: string
  }
  diffUi: {
    filterPlaceholder: string
    changedFilesTitle: string
    editedFilesOne: string
    editedFilesMany: string
    moreFiles: string
    showLess: string
    showMoreLines: string
    mergePr: string
    mergeStack: string
    mergeThrough: string
    guideFace: string
    guideChangesRow: string
    guideOtherChanges: string
    guideShowCompleteDiff: string
    closePr: string
    openOnGithub: string
    noChanges: string
    cardPreviewFiles: number
    collapseThresholdLines: number
    lineChunk: number
    inlineDiffMaxHeight: number
  }
  toolKind: Section & { editKinds: string[] }
  subscriberSource: Section
  pinKind: Section
  issueEventType: Section
  issueRelationType: Section & { forwardLabels: string[]; inverseLabels: string[] }
  issueRelationSource: Section
  sessionIssueSource: Section
  messageAuthorKind: Section
  conversationKind: Section
  messageCardKind: Section
  messageEventKind: Section
  codingAgent: Section
  codingModel: Section
  codingEffort: Section
  codexModel: Section
  codexEffort: Section
  deviceComputerUseDefaults: { model: string }
  actionInputType: Section
  mcpTransport: Section
  mcpAuth: Section
  // A device's agent defaults.
  deviceAgentDefaults: {
    model: string
    subagentModel: string
    threadModel: string
    codexModel: string
    codexThreadModel: string
  }
  actionInputs: { max: number; maxTextLength: number }
  startPrompt: { maxLength: number; maxImages: number }
  agentTrigger: {
    kindValues: string[]
    maxPerAgent: number
    cronMax: number
    tzMax: number
    catchUpHours: number
  }
  timeZones: { names: string[] }
  events: {
    builtinNames: string[]
    emitterValues: string[]
    nameMax: number
    payloadMaxBytes: number
    retentionDays: number
  }
  // EXP-1291: sessions.send's caps (one entrypoint ×4).
  sessionsSend: {
    textMax: number
    maxAttachments: number
    maxIssues: number
    briefMaxBytes: number
    messageBodyMax: number
    agentNameMax: number
    agentInstructionsMaxBytes: number
    treeMaxDepth: number
    treeMaxSessions: number
  }
  steerCommands: {
    commands: {
      name: string
      description: string
      argHint: string
      agents: string[]
      confirm: boolean
    }[]
  }
  // The MCP catalog (Settings › MCP servers' Add dialog, web + desktop): the
  // well-known hosted servers by id, display name, endpoint and the brand
  // mark slug (packages/icons `brand`). A `url` containing `{host}` is a
  // TEMPLATE (self-managed installs): never probed, the form opens on it.
  mcpCatalog: {
    servers: { id: string; name: string; url: string; mark: string }[]
  }
  expToolDisplay: {
    prefix: string
    resultKinds: string[]
    tools: {
      name: string
      title: string
      blurb: string
      progressive: string
      done: string
      progressiveMany: string
      doneMany: string
      subjectKey: string
      result: string
    }[]
  }
  turnState: Section
  // EXP-850/856: steer wire vocabulary shared by the engine, the relay and
  // every viewer — subagent edges, workflow cards, background tasks and the
  // claude-style working caption.
  subagentStatus: Section
  workflowAgentState: Section
  workflowStatus: Section
  backgroundTaskKind: Section
  taskListStatus: Section
  // EXP-1051: the context_layout segment catalog (keys, labels, tones).
  contextLayout: {
    title: string
    segments: { key: string; label: string; tone: string }[]
    derived: { key: string; label: string; tone: string }[]
    sources: string[]
    compactMinPercent: number
    charsPerToken: number
    detailMax: number
  }
  steerWorking: { verbs: string[]; tokenTickMs: number; previewMax: number }
  // EXP-920: the tool-answer chip kinds + the engine's per-tool preview spec.
  entityRefKind: Section
  expToolPreview: {
    maxRefs: number
    textMax: number
    tools: { name: string; refs: string }[]
  }
}

const contract: Contract = JSON.parse(
  readFileSync(join(__dirname, "..", "contract.json"), "utf8")
)

// Client-side liveness window for `running` coding_sessions rows, in ms —
// emitted pre-multiplied so no client does per-language time arithmetic.
const codingSessionStaleMs = contract.codingSession.staleHours * 60 * 60 * 1000

// EXP-481: how fresh a devices row's last_seen_at must be to render "online"
// (devices heartbeat ~30s; 90s = three missed beats). Pre-multiplied to ms.
const deviceOnlineWindowMs = contract.device.onlineWindowSeconds * 1000


// EXP-783: the steering transcript's budgets, window and page size — the
// same numbers on every client, so the byte-budget trim and the rendered
// window cannot drift apart per platform. EXP-786 adds the per-call diff
// caps (`toolDiffMaxLines`/`toolDiffMaxBytes`) the publisher truncates a
// `tool_update.diff` to before it rides the wire.
const steerFeed = contract.steerFeed

// EXP-916: the diff UI's shared copy and numbers — the ONE wording every
// diff surface uses (session edit cards, the Changes face, Reviews) ×4.
// Strings carry `{n}`/`{hidden}` placeholders the client substitutes.
const diffUi = contract.diffUi
const diffUiStrings: [string, string][] = [
  ["filterPlaceholder", diffUi.filterPlaceholder],
  ["changedFilesTitle", diffUi.changedFilesTitle],
  ["editedFilesOne", diffUi.editedFilesOne],
  ["editedFilesMany", diffUi.editedFilesMany],
  ["moreFiles", diffUi.moreFiles],
  ["showLess", diffUi.showLess],
  ["showMoreLines", diffUi.showMoreLines],
  ["mergePr", diffUi.mergePr],
  ["mergeStack", diffUi.mergeStack],
  ["mergeThrough", diffUi.mergeThrough],
  ["guideFace", diffUi.guideFace],
  ["guideChangesRow", diffUi.guideChangesRow],
  ["guideOtherChanges", diffUi.guideOtherChanges],
  ["guideShowCompleteDiff", diffUi.guideShowCompleteDiff],
  ["closePr", diffUi.closePr],
  ["openOnGithub", diffUi.openOnGithub],
  ["noChanges", diffUi.noChanges],
]
const diffUiInts: [string, number][] = [
  ["cardPreviewFiles", diffUi.cardPreviewFiles],
  ["collapseThresholdLines", diffUi.collapseThresholdLines],
  ["lineChunk", diffUi.lineChunk],
  ["inlineDiffMaxHeight", diffUi.inlineDiffMaxHeight],
]
const capFirst = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)
const screaming = (s: string) => s.replace(/([A-Z])/g, "_$1").toUpperCase()
const swiftDiffUi = [
  ...diffUiStrings.map(([k, v]) => `    public static let diffUi${capFirst(k)}: String = "${v}"`),
  ...diffUiInts.map(([k, v]) => `    public static let diffUi${capFirst(k)}: Int = ${v}`),
].join("\n")
const kotlinDiffUi = [
  ...diffUiStrings.map(([k, v]) => `    const val diffUi${capFirst(k)}: String = "${v}"`),
  ...diffUiInts.map(([k, v]) => `    const val diffUi${capFirst(k)}: Int = ${v}`),
].join("\n")
const rustDiffUi = [
  ...diffUiStrings.map(([k, v]) => `pub const DIFF_UI_${screaming(k)}: &str = "${v}";`),
  ...diffUiInts.map(([k, v]) => `pub const DIFF_UI_${screaming(k)}: usize = ${v};`),
].join("\n")

// EXP-1019: the launcher's copy — the verb in front of the subject chips and
// what the field asks for once the subject leads. One wording ×4.
const composerUi = contract.composerUi
const composerUiStrings: [string, string][] = [
  ["runHeadline", composerUi.runHeadline],
  ["implementHeadline", composerUi.implementHeadline],
  ["chatHeadline", composerUi.chatHeadline],
  ["chatPlaceholder", composerUi.chatPlaceholder],
  ["instructionsPlaceholder", composerUi.instructionsPlaceholder],
  ["dialogTitle", composerUi.dialogTitle],
  ["fixConflictsHeadline", composerUi.fixConflictsHeadline],
  ["fixConflictsSubmit", composerUi.fixConflictsSubmit],
  ["prPlaceholder", composerUi.prPlaceholder],
  ["conflictNote", composerUi.conflictNote],
]
const swiftComposerUi = composerUiStrings
  .map(([k, v]) => `    public static let composerUi${capFirst(k)}: String = "${v}"`)
  .join("\n")
const kotlinComposerUi = composerUiStrings
  .map(([k, v]) => `    const val composerUi${capFirst(k)}: String = "${v}"`)
  .join("\n")
const rustComposerUi = composerUiStrings
  .map(([k, v]) => `pub const COMPOSER_UI_${screaming(k)}: &str = "${v}";`)
  .join("\n")

// The 7 locked builtin issue statuses (EXP-314) — emitted as parallel arrays
// (keys/categories/names/colors/sortOrders) so every client can construct its
// local fallback status set without struct emission. Same order everywhere.
const statusDefaults = contract.issueStatusDefaults
const statusDefaultKeys = statusDefaults.map((d) => d.key)
const statusDefaultCategories = statusDefaults.map((d) => d.category)
const statusDefaultNames = statusDefaults.map((d) => d.name)
const statusDefaultColors = statusDefaults.map((d) => d.color)
const statusDefaultSortOrders = statusDefaults.map((d) => d.sortOrder)

// EXP-724: the curated steer slash commands, emitted as parallel arrays
// (names/descriptions/argHints/agents/confirm) like the status defaults.
// `agents` is a comma-joined list per row (no nested-array helper needed);
// clients split it to filter the menu by the session's agent.
const steerCommands = contract.steerCommands.commands
const steerCommandNames = steerCommands.map((c) => c.name)
const steerCommandDescriptions = steerCommands.map((c) => c.description)
const steerCommandArgHints = steerCommands.map((c) => c.argHint)
const steerCommandAgents = steerCommands.map((c) => c.agents.join(","))
const steerCommandConfirm = steerCommands.map((c) => c.confirm)

const mcpCatalog = contract.mcpCatalog.servers
const mcpCatalogIds = mcpCatalog.map((s) => s.id)
const mcpCatalogNames = mcpCatalog.map((s) => s.name)
const mcpCatalogUrls = mcpCatalog.map((s) => s.url)
const mcpCatalogMarks = mcpCatalog.map((s) => s.mark)

// EXP-1051: the context_layout catalog, flattened into parallel arrays.
const contextSegments = contract.contextLayout.segments
const contextSegmentKeys = contextSegments.map((s) => s.key)
const contextSegmentLabels = contextSegments.map((s) => s.label)
const contextSegmentTones = contextSegments.map((s) => s.tone)
const contextDerived = contract.contextLayout.derived
const contextDerivedKeys = contextDerived.map((s) => s.key)
const contextDerivedLabels = contextDerived.map((s) => s.label)
const contextDerivedTones = contextDerived.map((s) => s.tone)

// EXP-846: the Exponential MCP tool display table — one row per tool the
// server registers, emitted as parallel arrays (names/progressive/done/
// subjectKeys/results). A client strips `expToolPrefix` (and any MCP
// namespace before it) off a tool call's name, finds the row, and renders
// the brand mark + the progressive caption while the call runs, the `done`
// caption once it settles, the input field named by `subjectKey` as the
// subject, and a result preview keyed by `result` (an `expToolResultKinds`
// value). Unknown tools fall back to the raw name.
//
// EXP-862: `titles`/`blurbs` are the SETTINGS copy of the same rows — the
// built-in tools group of the MCP servers page (web + desktop) lists one row
// per tool as its title over a muted blurb, the wire name only as a tooltip.
//
// EXP-948: `progressiveMany`/`doneMany` are the same two captions for a RUN of
// consecutive calls to the SAME tool ("Reading {n} issues" / "Read {n} issues"),
// with `{n}` the member count — our tools group by themselves and never
// disappear into a generic "N other tools" fold.
const expTools = contract.expToolDisplay.tools
const expToolNames = expTools.map((t) => t.name)
const expToolTitles = expTools.map((t) => t.title)
const expToolBlurbs = expTools.map((t) => t.blurb)
const expToolProgressive = expTools.map((t) => t.progressive)
const expToolDone = expTools.map((t) => t.done)
const expToolProgressiveMany = expTools.map((t) => t.progressiveMany)
const expToolDoneMany = expTools.map((t) => t.doneMany)
const expToolSubjectKeys = expTools.map((t) => t.subjectKey)
// EXP-920: parallel arrays, tool name → preview spec (`kind@path …`).
const expToolPreviewNames = contract.expToolPreview.tools.map((t) => t.name)
const expToolPreviewRefs = contract.expToolPreview.tools.map((t) => t.refs)
const expToolResults = expTools.map((t) => t.result)

const HEADER_SWIFT = `// AUTO-GENERATED by packages/domain-contract/scripts/generate.ts — do not edit.
// Single source of truth: packages/domain-contract/contract.json.
`

const HEADER_KOTLIN = `// AUTO-GENERATED by packages/domain-contract/scripts/generate.ts — do not edit.
// Single source of truth: packages/domain-contract/contract.json.
`

function swiftStringArray(name: string, values: string[]): string {
  return `    public static let ${name}: [String] = [${values.map((v) => `"${v}"`).join(", ")}]`
}

function swiftIntArray(name: string, values: number[]): string {
  return `    public static let ${name}: [Int] = [${values.join(", ")}]`
}

function swiftBoolArray(name: string, values: boolean[]): string {
  return `    public static let ${name}: [Bool] = [${values.join(", ")}]`
}

function kotlinStringArray(name: string, values: string[]): string {
  return `    val ${name}: List<String> = listOf(${values.map((v) => `"${v}"`).join(", ")})`
}

function kotlinIntArray(name: string, values: number[]): string {
  return `    val ${name}: List<Int> = listOf(${values.join(", ")})`
}

function kotlinBoolArray(name: string, values: boolean[]): string {
  return `    val ${name}: List<Boolean> = listOf(${values.join(", ")})`
}

function camelCase(s: string): string {
  return s.replace(/_([a-z])/g, (_, ch) => ch.toUpperCase())
}

function pascalCase(s: string): string {
  const c = camelCase(s)
  return c.charAt(0).toUpperCase() + c.slice(1)
}

// Emits per-value named constants so call sites can reference
// `DomainContract.teamRoleOwner` instead of the bare string literal.
function swiftNamedValues(prefix: string, values: string[]): string {
  return values
    .map(
      (v) =>
        `    public static let ${prefix}${pascalCase(v)}: String = "${v}"`
    )
    .join(`\n`)
}

function kotlinNamedValues(prefix: string, values: string[]): string {
  return values
    .map((v) => `    const val ${prefix}${pascalCase(v)}: String = "${v}"`)
    .join(`\n`)
}

// --- Rust (consumed by apps/desktop) ---

function screamingSnake(s: string): string {
  return s
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2") // camelCase word breaks
    .replace(/[^A-Za-z0-9]+/g, "_") // non-alnum → _
    .toUpperCase()
}

function rustStrSlice(name: string, values: string[]): string {
  return `pub const ${screamingSnake(name)}: &[&str] = &[${values
    .map((v) => `"${v}"`)
    .join(", ")}];`
}

function rustIntSlice(name: string, values: number[]): string {
  return `pub const ${screamingSnake(name)}: &[i32] = &[${values.join(", ")}];`
}

function rustBoolSlice(name: string, values: boolean[]): string {
  return `pub const ${screamingSnake(name)}: &[bool] = &[${values.join(", ")}];`
}

function rustNamedValues(prefix: string, values: string[]): string {
  return values
    .map(
      (v) =>
        `pub const ${screamingSnake(prefix)}_${screamingSnake(v)}: &str = "${v}";`
    )
    .join(`\n`)
}

const swift = `${HEADER_SWIFT}
import Foundation

public enum DomainContract {
${swiftStringArray("issueStatusValues", contract.issueStatus.values)}
${swiftStringArray("issueStatusDisplayOrder", contract.issueStatus.displayOrder ?? contract.issueStatus.values)}
${swiftStringArray("issueStatusCategoryValues", contract.issueStatusCategory.values)}
${swiftStringArray("issueStatusCategoryDisplayOrder", contract.issueStatusCategory.displayOrder ?? contract.issueStatusCategory.values)}
${swiftStringArray("issueStatusDefaultKeys", statusDefaultKeys)}
${swiftStringArray("issueStatusDefaultCategories", statusDefaultCategories)}
${swiftStringArray("issueStatusDefaultNames", statusDefaultNames)}
${swiftStringArray("issueStatusDefaultColors", statusDefaultColors)}
${swiftIntArray("issueStatusDefaultSortOrders", statusDefaultSortOrders)}
${swiftStringArray("issuePriorityValues", contract.issuePriority.values)}
${swiftStringArray("issuePriorityDisplayOrder", contract.issuePriority.displayOrder ?? contract.issuePriority.values)}
${swiftStringArray("issueSourceValues", contract.issueSource.values)}
${swiftStringArray("issueEstimationValues", contract.issueEstimation.values)}
${swiftStringArray("teamRoleValues", contract.teamRole.values)}
${swiftStringArray("boardIconValues", contract.boardIcon.values)}
${swiftStringArray("deviceIconValues", contract.deviceIcon.values)}
${swiftStringArray("commentKindValues", contract.commentKind.values)}
${swiftStringArray("commentSourceValues", contract.commentSource.values)}
${swiftStringArray("commentAudienceValues", contract.commentAudience.values)}
${swiftStringArray("notificationTypeValues", contract.notificationType.values)}
${swiftStringArray("prStateValues", contract.prState.values)}
${swiftStringArray("sessionKindValues", contract.sessionKind.values)}
${swiftStringArray("sessionStatusValues", contract.sessionStatus.values)}
${swiftStringArray("codingSessionBlockedKinds", contract.codingSessionBlocked.kinds)}
${swiftStringArray("codingSessionBlockedWindows", contract.codingSessionBlocked.windows)}
${swiftStringArray("toolKindValues", contract.toolKind.values)}
${swiftStringArray("toolKindEditValues", contract.toolKind.editKinds)}
${swiftStringArray("subscriberSourceValues", contract.subscriberSource.values)}
${swiftStringArray("pinKindValues", contract.pinKind.values)}
${swiftStringArray("issueEventTypeValues", contract.issueEventType.values)}
${swiftStringArray("issueRelationTypeValues", contract.issueRelationType.values)}
${swiftStringArray("issueRelationTypeForwardLabels", contract.issueRelationType.forwardLabels)}
${swiftStringArray("issueRelationTypeInverseLabels", contract.issueRelationType.inverseLabels)}
${swiftStringArray("issueRelationSourceValues", contract.issueRelationSource.values)}
${swiftStringArray("sessionIssueSourceValues", contract.sessionIssueSource.values)}
${swiftStringArray("messageAuthorKindValues", contract.messageAuthorKind.values)}
${swiftStringArray("conversationKindValues", contract.conversationKind.values)}
${swiftStringArray("messageCardKindValues", contract.messageCardKind.values)}
${swiftStringArray("messageEventKindValues", contract.messageEventKind.values)}
${swiftStringArray("codingAgentValues", contract.codingAgent.values)}
${swiftStringArray("codingModelValues", contract.codingModel.values)}
${swiftStringArray("codingEffortValues", contract.codingEffort.values)}
${swiftStringArray("codexModelValues", contract.codexModel.values)}
${swiftStringArray("codexEffortValues", contract.codexEffort.values)}
${swiftStringArray("actionInputTypeValues", contract.actionInputType.values)}
${swiftStringArray("mcpTransportValues", contract.mcpTransport.values)}
${swiftStringArray("mcpAuthValues", contract.mcpAuth.values)}
${swiftStringArray("agentTriggerKindValues", contract.agentTrigger.kindValues)}
${swiftStringArray("timeZoneNames", contract.timeZones.names)}
${swiftStringArray("eventBuiltinNames", contract.events.builtinNames)}
${swiftStringArray("eventEmitterValues", contract.events.emitterValues)}
${swiftStringArray("steerCommandNames", steerCommandNames)}
${swiftStringArray("steerCommandDescriptions", steerCommandDescriptions)}
${swiftStringArray("steerCommandArgHints", steerCommandArgHints)}
${swiftStringArray("steerCommandAgents", steerCommandAgents)}
${swiftBoolArray("steerCommandConfirm", steerCommandConfirm)}
${swiftStringArray("mcpCatalogIds", mcpCatalogIds)}
${swiftStringArray("mcpCatalogNames", mcpCatalogNames)}
${swiftStringArray("mcpCatalogUrls", mcpCatalogUrls)}
${swiftStringArray("mcpCatalogMarks", mcpCatalogMarks)}
    public static let expToolPrefix: String = "${contract.expToolDisplay.prefix}"
${swiftStringArray("expToolResultKinds", contract.expToolDisplay.resultKinds)}
${swiftStringArray("expToolNames", expToolNames)}
${swiftStringArray("expToolTitles", expToolTitles)}
${swiftStringArray("expToolBlurbs", expToolBlurbs)}
${swiftStringArray("expToolProgressive", expToolProgressive)}
${swiftStringArray("expToolDone", expToolDone)}
${swiftStringArray("expToolProgressiveMany", expToolProgressiveMany)}
${swiftStringArray("expToolDoneMany", expToolDoneMany)}
${swiftStringArray("expToolSubjectKeys", expToolSubjectKeys)}
${swiftStringArray("expToolResults", expToolResults)}
${swiftStringArray("turnStateValues", contract.turnState.values)}
${swiftStringArray("entityRefKindValues", contract.entityRefKind.values)}
    public static let expToolPreviewMaxRefs: Int = ${contract.expToolPreview.maxRefs}
    public static let expToolPreviewTextMax: Int = ${contract.expToolPreview.textMax}
${swiftStringArray("subagentStatusValues", contract.subagentStatus.values)}
${swiftStringArray("workflowAgentStateValues", contract.workflowAgentState.values)}
${swiftStringArray("workflowStatusValues", contract.workflowStatus.values)}
${swiftStringArray("backgroundTaskKindValues", contract.backgroundTaskKind.values)}
${swiftStringArray("taskListStatusValues", contract.taskListStatus.values)}
${swiftStringArray("contextLayoutSegmentKeys", contextSegmentKeys)}
${swiftStringArray("contextLayoutSegmentLabels", contextSegmentLabels)}
${swiftStringArray("contextLayoutSegmentTones", contextSegmentTones)}
${swiftStringArray("contextLayoutDerivedKeys", contextDerivedKeys)}
${swiftStringArray("contextLayoutDerivedLabels", contextDerivedLabels)}
${swiftStringArray("contextLayoutDerivedTones", contextDerivedTones)}
${swiftStringArray("contextLayoutSourceValues", contract.contextLayout.sources)}
    public static let contextLayoutTitle: String = "${contract.contextLayout.title}"
    public static let contextLayoutCompactMinPercent: Int = ${contract.contextLayout.compactMinPercent}
    public static let contextLayoutCharsPerToken: Int = ${contract.contextLayout.charsPerToken}
    public static let contextLayoutDetailMax: Int = ${contract.contextLayout.detailMax}
${swiftStringArray("steerWorkingVerbs", contract.steerWorking.verbs)}
    public static let steerWorkingTokenTickMs: Int = ${contract.steerWorking.tokenTickMs}
    public static let steerWorkingPreviewMax: Int = ${contract.steerWorking.previewMax}

    public static let issueStatusStartedMax: Int = ${contract.issueStatusCategory.startedMax}
    public static let codingSessionStaleMs: Int = ${codingSessionStaleMs}
${swiftStringArray("codingSessionLaunchKeys", contract.codingSession.launchKeys)}
    public static let codingSessionComputerUseCap: String = "${contract.codingSession.computerUseCap}"
    public static let deviceOnlineWindowMs: Int = ${deviceOnlineWindowMs}
    public static let sessionsSendTextMax: Int = ${contract.sessionsSend.textMax}
    public static let sessionsSendMaxAttachments: Int = ${contract.sessionsSend.maxAttachments}
    public static let sessionsSendMaxIssues: Int = ${contract.sessionsSend.maxIssues}
    public static let sessionsSendBriefMaxBytes: Int = ${contract.sessionsSend.briefMaxBytes}
    public static let sessionsSendMessageBodyMax: Int = ${contract.sessionsSend.messageBodyMax}
    public static let sessionsSendAgentNameMax: Int = ${contract.sessionsSend.agentNameMax}
    public static let sessionsSendAgentInstructionsMaxBytes: Int = ${contract.sessionsSend.agentInstructionsMaxBytes}
    public static let sessionsSendTreeMaxDepth: Int = ${contract.sessionsSend.treeMaxDepth}
    public static let sessionsSendTreeMaxSessions: Int = ${contract.sessionsSend.treeMaxSessions}
    public static let deviceAgentDefaultsModel: String = "${contract.deviceAgentDefaults.model}"
    public static let deviceAgentDefaultsSubagentModel: String = "${contract.deviceAgentDefaults.subagentModel}"
    public static let deviceAgentDefaultsThreadModel: String = "${contract.deviceAgentDefaults.threadModel}"
    public static let deviceAgentDefaultsCodexModel: String = "${contract.deviceAgentDefaults.codexModel}"
    public static let deviceAgentDefaultsCodexThreadModel: String = "${contract.deviceAgentDefaults.codexThreadModel}"
    public static let deviceComputerUseDefaultsModel: String = "${contract.deviceComputerUseDefaults.model}"
    public static let actionInputsMax: Int = ${contract.actionInputs.max}
    public static let actionInputTextMax: Int = ${contract.actionInputs.maxTextLength}
    public static let startPromptMaxLength: Int = ${contract.startPrompt.maxLength}
    public static let startPromptMaxImages: Int = ${contract.startPrompt.maxImages}
    public static let agentTriggerMaxPerAgent: Int = ${contract.agentTrigger.maxPerAgent}
    public static let agentTriggerCronMax: Int = ${contract.agentTrigger.cronMax}
    public static let agentTriggerTzMax: Int = ${contract.agentTrigger.tzMax}
    public static let agentTriggerCatchUpHours: Int = ${contract.agentTrigger.catchUpHours}
    public static let eventNameMax: Int = ${contract.events.nameMax}
    public static let eventPayloadMaxBytes: Int = ${contract.events.payloadMaxBytes}
    public static let eventRetentionDays: Int = ${contract.events.retentionDays}
    public static let steerFeedByteCap: Int = ${steerFeed.byteCap}
    public static let steerFeedItemCap: Int = ${steerFeed.itemCap}
    public static let steerFeedTrimTargetPercent: Int = ${steerFeed.trimTargetPercent}
    public static let steerFeedItemOverheadBytes: Int = ${steerFeed.itemOverheadBytes}
    public static let steerFeedWindow: Int = ${steerFeed.window}
    public static let steerFeedWindowStep: Int = ${steerFeed.windowStep}
    public static let steerFeedHistoryPageMax: Int = ${steerFeed.historyPageMax}
    public static let steerFeedToolDiffMaxLines: Int = ${steerFeed.toolDiffMaxLines}
    public static let steerFeedToolDiffMaxBytes: Int = ${steerFeed.toolDiffMaxBytes}
    public static let steerFeedToolOutputMaxLines: Int = ${steerFeed.toolOutputMaxLines}
    public static let steerFeedToolOutputMaxBytes: Int = ${steerFeed.toolOutputMaxBytes}
${swiftDiffUi}
${swiftComposerUi}

${swiftNamedValues("issueStatusCategory", contract.issueStatusCategory.values)}
${swiftNamedValues("issueSource", contract.issueSource.values)}
${swiftNamedValues("issueEstimation", contract.issueEstimation.values)}
${swiftNamedValues("teamRole", contract.teamRole.values)}
${swiftNamedValues("commentKind", contract.commentKind.values)}
${swiftNamedValues("commentSource", contract.commentSource.values)}
${swiftNamedValues("commentAudience", contract.commentAudience.values)}
${swiftNamedValues("notificationType", contract.notificationType.values)}
${swiftNamedValues("prState", contract.prState.values)}
${swiftNamedValues("sessionKind", contract.sessionKind.values)}
${swiftNamedValues("sessionStatus", contract.sessionStatus.values)}
${swiftNamedValues("subscriberSource", contract.subscriberSource.values)}
${swiftNamedValues("pinKind", contract.pinKind.values)}
${swiftNamedValues("issueEventType", contract.issueEventType.values)}
${swiftNamedValues("issueRelationType", contract.issueRelationType.values)}
${swiftNamedValues("issueRelationSource", contract.issueRelationSource.values)}
${swiftNamedValues("sessionIssueSource", contract.sessionIssueSource.values)}
${swiftNamedValues("messageAuthorKind", contract.messageAuthorKind.values)}
${swiftNamedValues("conversationKind", contract.conversationKind.values)}
${swiftNamedValues("messageCardKind", contract.messageCardKind.values)}
${swiftNamedValues("messageEventKind", contract.messageEventKind.values)}
}
`

const kotlin = `${HEADER_KOTLIN}package com.exponential.app.domain

object DomainContract {
${kotlinStringArray("issueStatusValues", contract.issueStatus.values)}
${kotlinStringArray("issueStatusDisplayOrder", contract.issueStatus.displayOrder ?? contract.issueStatus.values)}
${kotlinStringArray("issueStatusCategoryValues", contract.issueStatusCategory.values)}
${kotlinStringArray("issueStatusCategoryDisplayOrder", contract.issueStatusCategory.displayOrder ?? contract.issueStatusCategory.values)}
${kotlinStringArray("issueStatusDefaultKeys", statusDefaultKeys)}
${kotlinStringArray("issueStatusDefaultCategories", statusDefaultCategories)}
${kotlinStringArray("issueStatusDefaultNames", statusDefaultNames)}
${kotlinStringArray("issueStatusDefaultColors", statusDefaultColors)}
${kotlinIntArray("issueStatusDefaultSortOrders", statusDefaultSortOrders)}
${kotlinStringArray("issuePriorityValues", contract.issuePriority.values)}
${kotlinStringArray("issuePriorityDisplayOrder", contract.issuePriority.displayOrder ?? contract.issuePriority.values)}
${kotlinStringArray("issueSourceValues", contract.issueSource.values)}
${kotlinStringArray("issueEstimationValues", contract.issueEstimation.values)}
${kotlinStringArray("teamRoleValues", contract.teamRole.values)}
${kotlinStringArray("boardIconValues", contract.boardIcon.values)}
${kotlinStringArray("deviceIconValues", contract.deviceIcon.values)}
${kotlinStringArray("commentKindValues", contract.commentKind.values)}
${kotlinStringArray("commentSourceValues", contract.commentSource.values)}
${kotlinStringArray("commentAudienceValues", contract.commentAudience.values)}
${kotlinStringArray("notificationTypeValues", contract.notificationType.values)}
${kotlinStringArray("prStateValues", contract.prState.values)}
${kotlinStringArray("sessionKindValues", contract.sessionKind.values)}
${kotlinStringArray("sessionStatusValues", contract.sessionStatus.values)}
${kotlinStringArray("codingSessionBlockedKinds", contract.codingSessionBlocked.kinds)}
${kotlinStringArray("codingSessionBlockedWindows", contract.codingSessionBlocked.windows)}
${kotlinStringArray("toolKindValues", contract.toolKind.values)}
${kotlinStringArray("toolKindEditValues", contract.toolKind.editKinds)}
${kotlinStringArray("subscriberSourceValues", contract.subscriberSource.values)}
${kotlinStringArray("pinKindValues", contract.pinKind.values)}
${kotlinStringArray("issueEventTypeValues", contract.issueEventType.values)}
${kotlinStringArray("issueRelationTypeValues", contract.issueRelationType.values)}
${kotlinStringArray("issueRelationTypeForwardLabels", contract.issueRelationType.forwardLabels)}
${kotlinStringArray("issueRelationTypeInverseLabels", contract.issueRelationType.inverseLabels)}
${kotlinStringArray("issueRelationSourceValues", contract.issueRelationSource.values)}
${kotlinStringArray("sessionIssueSourceValues", contract.sessionIssueSource.values)}
${kotlinStringArray("messageAuthorKindValues", contract.messageAuthorKind.values)}
${kotlinStringArray("conversationKindValues", contract.conversationKind.values)}
${kotlinStringArray("messageCardKindValues", contract.messageCardKind.values)}
${kotlinStringArray("messageEventKindValues", contract.messageEventKind.values)}
${kotlinStringArray("codingAgentValues", contract.codingAgent.values)}
${kotlinStringArray("codingModelValues", contract.codingModel.values)}
${kotlinStringArray("codingEffortValues", contract.codingEffort.values)}
${kotlinStringArray("codexModelValues", contract.codexModel.values)}
${kotlinStringArray("codexEffortValues", contract.codexEffort.values)}
${kotlinStringArray("actionInputTypeValues", contract.actionInputType.values)}
${kotlinStringArray("mcpTransportValues", contract.mcpTransport.values)}
${kotlinStringArray("mcpAuthValues", contract.mcpAuth.values)}
${kotlinStringArray("agentTriggerKindValues", contract.agentTrigger.kindValues)}
${kotlinStringArray("timeZoneNames", contract.timeZones.names)}
${kotlinStringArray("eventBuiltinNames", contract.events.builtinNames)}
${kotlinStringArray("eventEmitterValues", contract.events.emitterValues)}
${kotlinStringArray("steerCommandNames", steerCommandNames)}
${kotlinStringArray("steerCommandDescriptions", steerCommandDescriptions)}
${kotlinStringArray("steerCommandArgHints", steerCommandArgHints)}
${kotlinStringArray("steerCommandAgents", steerCommandAgents)}
${kotlinBoolArray("steerCommandConfirm", steerCommandConfirm)}
${kotlinStringArray("mcpCatalogIds", mcpCatalogIds)}
${kotlinStringArray("mcpCatalogNames", mcpCatalogNames)}
${kotlinStringArray("mcpCatalogUrls", mcpCatalogUrls)}
${kotlinStringArray("mcpCatalogMarks", mcpCatalogMarks)}
    const val expToolPrefix: String = "${contract.expToolDisplay.prefix}"
${kotlinStringArray("expToolResultKinds", contract.expToolDisplay.resultKinds)}
${kotlinStringArray("expToolNames", expToolNames)}
${kotlinStringArray("expToolTitles", expToolTitles)}
${kotlinStringArray("expToolBlurbs", expToolBlurbs)}
${kotlinStringArray("expToolProgressive", expToolProgressive)}
${kotlinStringArray("expToolDone", expToolDone)}
${kotlinStringArray("expToolProgressiveMany", expToolProgressiveMany)}
${kotlinStringArray("expToolDoneMany", expToolDoneMany)}
${kotlinStringArray("expToolSubjectKeys", expToolSubjectKeys)}
${kotlinStringArray("expToolResults", expToolResults)}
${kotlinStringArray("turnStateValues", contract.turnState.values)}
${kotlinStringArray("entityRefKindValues", contract.entityRefKind.values)}
    const val expToolPreviewMaxRefs: Int = ${contract.expToolPreview.maxRefs}
    const val expToolPreviewTextMax: Int = ${contract.expToolPreview.textMax}
${kotlinStringArray("subagentStatusValues", contract.subagentStatus.values)}
${kotlinStringArray("workflowAgentStateValues", contract.workflowAgentState.values)}
${kotlinStringArray("workflowStatusValues", contract.workflowStatus.values)}
${kotlinStringArray("backgroundTaskKindValues", contract.backgroundTaskKind.values)}
${kotlinStringArray("taskListStatusValues", contract.taskListStatus.values)}
${kotlinStringArray("contextLayoutSegmentKeys", contextSegmentKeys)}
${kotlinStringArray("contextLayoutSegmentLabels", contextSegmentLabels)}
${kotlinStringArray("contextLayoutSegmentTones", contextSegmentTones)}
${kotlinStringArray("contextLayoutDerivedKeys", contextDerivedKeys)}
${kotlinStringArray("contextLayoutDerivedLabels", contextDerivedLabels)}
${kotlinStringArray("contextLayoutDerivedTones", contextDerivedTones)}
${kotlinStringArray("contextLayoutSourceValues", contract.contextLayout.sources)}
    const val contextLayoutTitle: String = "${contract.contextLayout.title}"
    const val contextLayoutCompactMinPercent: Int = ${contract.contextLayout.compactMinPercent}
    const val contextLayoutCharsPerToken: Int = ${contract.contextLayout.charsPerToken}
    const val contextLayoutDetailMax: Int = ${contract.contextLayout.detailMax}
${kotlinStringArray("steerWorkingVerbs", contract.steerWorking.verbs)}
    const val steerWorkingTokenTickMs: Long = ${contract.steerWorking.tokenTickMs}L
    const val steerWorkingPreviewMax: Int = ${contract.steerWorking.previewMax}

    const val issueStatusStartedMax: Int = ${contract.issueStatusCategory.startedMax}
    const val codingSessionStaleMs: Long = ${codingSessionStaleMs}L
${kotlinStringArray("codingSessionLaunchKeys", contract.codingSession.launchKeys)}
    const val codingSessionComputerUseCap: String = "${contract.codingSession.computerUseCap}"
    const val deviceOnlineWindowMs: Long = ${deviceOnlineWindowMs}L
    const val sessionsSendTextMax: Int = ${contract.sessionsSend.textMax}
    const val sessionsSendMaxAttachments: Int = ${contract.sessionsSend.maxAttachments}
    const val sessionsSendMaxIssues: Int = ${contract.sessionsSend.maxIssues}
    const val sessionsSendBriefMaxBytes: Int = ${contract.sessionsSend.briefMaxBytes}
    const val sessionsSendMessageBodyMax: Int = ${contract.sessionsSend.messageBodyMax}
    const val sessionsSendAgentNameMax: Int = ${contract.sessionsSend.agentNameMax}
    const val sessionsSendAgentInstructionsMaxBytes: Int = ${contract.sessionsSend.agentInstructionsMaxBytes}
    const val sessionsSendTreeMaxDepth: Int = ${contract.sessionsSend.treeMaxDepth}
    const val sessionsSendTreeMaxSessions: Int = ${contract.sessionsSend.treeMaxSessions}
    const val deviceAgentDefaultsModel: String = "${contract.deviceAgentDefaults.model}"
    const val deviceAgentDefaultsSubagentModel: String = "${contract.deviceAgentDefaults.subagentModel}"
    const val deviceAgentDefaultsThreadModel: String = "${contract.deviceAgentDefaults.threadModel}"
    const val deviceAgentDefaultsCodexModel: String = "${contract.deviceAgentDefaults.codexModel}"
    const val deviceAgentDefaultsCodexThreadModel: String = "${contract.deviceAgentDefaults.codexThreadModel}"
    const val deviceComputerUseDefaultsModel: String = "${contract.deviceComputerUseDefaults.model}"
    const val actionInputsMax: Int = ${contract.actionInputs.max}
    const val actionInputTextMax: Int = ${contract.actionInputs.maxTextLength}
    const val startPromptMaxLength: Int = ${contract.startPrompt.maxLength}
    const val startPromptMaxImages: Int = ${contract.startPrompt.maxImages}
    const val agentTriggerMaxPerAgent: Int = ${contract.agentTrigger.maxPerAgent}
    const val agentTriggerCronMax: Int = ${contract.agentTrigger.cronMax}
    const val agentTriggerTzMax: Int = ${contract.agentTrigger.tzMax}
    const val agentTriggerCatchUpHours: Int = ${contract.agentTrigger.catchUpHours}
    const val eventNameMax: Int = ${contract.events.nameMax}
    const val eventPayloadMaxBytes: Int = ${contract.events.payloadMaxBytes}
    const val eventRetentionDays: Int = ${contract.events.retentionDays}
    const val steerFeedByteCap: Long = ${steerFeed.byteCap}L
    const val steerFeedItemCap: Int = ${steerFeed.itemCap}
    const val steerFeedTrimTargetPercent: Int = ${steerFeed.trimTargetPercent}
    const val steerFeedItemOverheadBytes: Long = ${steerFeed.itemOverheadBytes}L
    const val steerFeedWindow: Int = ${steerFeed.window}
    const val steerFeedWindowStep: Int = ${steerFeed.windowStep}
    const val steerFeedHistoryPageMax: Int = ${steerFeed.historyPageMax}
    const val steerFeedToolDiffMaxLines: Int = ${steerFeed.toolDiffMaxLines}
    const val steerFeedToolDiffMaxBytes: Int = ${steerFeed.toolDiffMaxBytes}
    const val steerFeedToolOutputMaxLines: Int = ${steerFeed.toolOutputMaxLines}
    const val steerFeedToolOutputMaxBytes: Int = ${steerFeed.toolOutputMaxBytes}
${kotlinDiffUi}
${kotlinComposerUi}

${kotlinNamedValues("issueStatusCategory", contract.issueStatusCategory.values)}
${kotlinNamedValues("issueSource", contract.issueSource.values)}
${kotlinNamedValues("issueEstimation", contract.issueEstimation.values)}
${kotlinNamedValues("teamRole", contract.teamRole.values)}
${kotlinNamedValues("commentKind", contract.commentKind.values)}
${kotlinNamedValues("commentSource", contract.commentSource.values)}
${kotlinNamedValues("commentAudience", contract.commentAudience.values)}
${kotlinNamedValues("notificationType", contract.notificationType.values)}
${kotlinNamedValues("prState", contract.prState.values)}
${kotlinNamedValues("sessionKind", contract.sessionKind.values)}
${kotlinNamedValues("sessionStatus", contract.sessionStatus.values)}
${kotlinNamedValues("subscriberSource", contract.subscriberSource.values)}
${kotlinNamedValues("pinKind", contract.pinKind.values)}
${kotlinNamedValues("issueEventType", contract.issueEventType.values)}
${kotlinNamedValues("issueRelationType", contract.issueRelationType.values)}
${kotlinNamedValues("issueRelationSource", contract.issueRelationSource.values)}
${kotlinNamedValues("sessionIssueSource", contract.sessionIssueSource.values)}
${kotlinNamedValues("messageAuthorKind", contract.messageAuthorKind.values)}
${kotlinNamedValues("conversationKind", contract.conversationKind.values)}
${kotlinNamedValues("messageCardKind", contract.messageCardKind.values)}
${kotlinNamedValues("messageEventKind", contract.messageEventKind.values)}
}
`

const HEADER_RUST = `// AUTO-GENERATED by packages/domain-contract/scripts/generate.ts — do not edit.
// Single source of truth: packages/domain-contract/contract.json.
`

const rust = `${HEADER_RUST}
${rustStrSlice("issueStatusValues", contract.issueStatus.values)}
${rustStrSlice("issueStatusDisplayOrder", contract.issueStatus.displayOrder ?? contract.issueStatus.values)}
${rustStrSlice("issueStatusCategoryValues", contract.issueStatusCategory.values)}
${rustStrSlice("issueStatusCategoryDisplayOrder", contract.issueStatusCategory.displayOrder ?? contract.issueStatusCategory.values)}
${rustStrSlice("issueStatusDefaultKeys", statusDefaultKeys)}
${rustStrSlice("issueStatusDefaultCategories", statusDefaultCategories)}
${rustStrSlice("issueStatusDefaultNames", statusDefaultNames)}
${rustStrSlice("issueStatusDefaultColors", statusDefaultColors)}
${rustIntSlice("issueStatusDefaultSortOrders", statusDefaultSortOrders)}
${rustStrSlice("issuePriorityValues", contract.issuePriority.values)}
${rustStrSlice("issuePriorityDisplayOrder", contract.issuePriority.displayOrder ?? contract.issuePriority.values)}
${rustStrSlice("issueSourceValues", contract.issueSource.values)}
${rustStrSlice("issueEstimationValues", contract.issueEstimation.values)}
${rustStrSlice("teamRoleValues", contract.teamRole.values)}
${rustStrSlice("boardIconValues", contract.boardIcon.values)}
${rustStrSlice("deviceIconValues", contract.deviceIcon.values)}
${rustStrSlice("commentKindValues", contract.commentKind.values)}
${rustStrSlice("commentSourceValues", contract.commentSource.values)}
${rustStrSlice("commentAudienceValues", contract.commentAudience.values)}
${rustStrSlice("notificationTypeValues", contract.notificationType.values)}
${rustStrSlice("prStateValues", contract.prState.values)}
${rustStrSlice("sessionKindValues", contract.sessionKind.values)}
${rustStrSlice("sessionStatusValues", contract.sessionStatus.values)}
${rustStrSlice("codingSessionBlockedKinds", contract.codingSessionBlocked.kinds)}
${rustStrSlice("codingSessionBlockedWindows", contract.codingSessionBlocked.windows)}
${rustStrSlice("toolKindValues", contract.toolKind.values)}
${rustStrSlice("toolKindEditValues", contract.toolKind.editKinds)}
${rustStrSlice("subscriberSourceValues", contract.subscriberSource.values)}
${rustStrSlice("pinKindValues", contract.pinKind.values)}
${rustStrSlice("issueEventTypeValues", contract.issueEventType.values)}
${rustStrSlice("issueRelationTypeValues", contract.issueRelationType.values)}
${rustStrSlice("issueRelationTypeForwardLabels", contract.issueRelationType.forwardLabels)}
${rustStrSlice("issueRelationTypeInverseLabels", contract.issueRelationType.inverseLabels)}
${rustStrSlice("issueRelationSourceValues", contract.issueRelationSource.values)}
${rustStrSlice("sessionIssueSourceValues", contract.sessionIssueSource.values)}
${rustStrSlice("messageAuthorKindValues", contract.messageAuthorKind.values)}
${rustStrSlice("conversationKindValues", contract.conversationKind.values)}
${rustStrSlice("messageCardKindValues", contract.messageCardKind.values)}
${rustStrSlice("messageEventKindValues", contract.messageEventKind.values)}
${rustStrSlice("codingAgentValues", contract.codingAgent.values)}
${rustStrSlice("codingModelValues", contract.codingModel.values)}
${rustStrSlice("codingEffortValues", contract.codingEffort.values)}
${rustStrSlice("codexModelValues", contract.codexModel.values)}
${rustStrSlice("codexEffortValues", contract.codexEffort.values)}
${rustStrSlice("actionInputTypeValues", contract.actionInputType.values)}
${rustStrSlice("mcpTransportValues", contract.mcpTransport.values)}
${rustStrSlice("mcpAuthValues", contract.mcpAuth.values)}
${rustStrSlice("agentTriggerKindValues", contract.agentTrigger.kindValues)}
${rustStrSlice("timeZoneNames", contract.timeZones.names)}
${rustStrSlice("eventBuiltinNames", contract.events.builtinNames)}
${rustStrSlice("eventEmitterValues", contract.events.emitterValues)}
${rustStrSlice("steerCommandNames", steerCommandNames)}
${rustStrSlice("steerCommandDescriptions", steerCommandDescriptions)}
${rustStrSlice("steerCommandArgHints", steerCommandArgHints)}
${rustStrSlice("steerCommandAgents", steerCommandAgents)}
${rustBoolSlice("steerCommandConfirm", steerCommandConfirm)}
${rustStrSlice("mcpCatalogIds", mcpCatalogIds)}
${rustStrSlice("mcpCatalogNames", mcpCatalogNames)}
${rustStrSlice("mcpCatalogUrls", mcpCatalogUrls)}
${rustStrSlice("mcpCatalogMarks", mcpCatalogMarks)}
pub const EXP_TOOL_PREFIX: &str = "${contract.expToolDisplay.prefix}";
${rustStrSlice("expToolResultKinds", contract.expToolDisplay.resultKinds)}
${rustStrSlice("expToolNames", expToolNames)}
${rustStrSlice("expToolTitles", expToolTitles)}
${rustStrSlice("expToolBlurbs", expToolBlurbs)}
${rustStrSlice("expToolProgressive", expToolProgressive)}
${rustStrSlice("expToolDone", expToolDone)}
${rustStrSlice("expToolProgressiveMany", expToolProgressiveMany)}
${rustStrSlice("expToolDoneMany", expToolDoneMany)}
${rustStrSlice("expToolSubjectKeys", expToolSubjectKeys)}
${rustStrSlice("expToolResults", expToolResults)}
${rustStrSlice("turnStateValues", contract.turnState.values)}
${rustStrSlice("entityRefKindValues", contract.entityRefKind.values)}
${rustStrSlice("expToolPreviewNames", expToolPreviewNames)}
${rustStrSlice("expToolPreviewRefs", expToolPreviewRefs)}
pub const EXP_TOOL_PREVIEW_MAX_REFS: usize = ${contract.expToolPreview.maxRefs};
pub const EXP_TOOL_PREVIEW_TEXT_MAX: usize = ${contract.expToolPreview.textMax};
${rustStrSlice("subagentStatusValues", contract.subagentStatus.values)}
${rustStrSlice("workflowAgentStateValues", contract.workflowAgentState.values)}
${rustStrSlice("workflowStatusValues", contract.workflowStatus.values)}
${rustStrSlice("backgroundTaskKindValues", contract.backgroundTaskKind.values)}
${rustStrSlice("taskListStatusValues", contract.taskListStatus.values)}
${rustStrSlice("contextLayoutSegmentKeys", contextSegmentKeys)}
${rustStrSlice("contextLayoutSegmentLabels", contextSegmentLabels)}
${rustStrSlice("contextLayoutSegmentTones", contextSegmentTones)}
${rustStrSlice("contextLayoutDerivedKeys", contextDerivedKeys)}
${rustStrSlice("contextLayoutDerivedLabels", contextDerivedLabels)}
${rustStrSlice("contextLayoutDerivedTones", contextDerivedTones)}
${rustStrSlice("contextLayoutSourceValues", contract.contextLayout.sources)}
pub const CONTEXT_LAYOUT_TITLE: &str = "${contract.contextLayout.title}";
pub const CONTEXT_LAYOUT_COMPACT_MIN_PERCENT: u8 = ${contract.contextLayout.compactMinPercent};
pub const CONTEXT_LAYOUT_CHARS_PER_TOKEN: usize = ${contract.contextLayout.charsPerToken};
pub const CONTEXT_LAYOUT_DETAIL_MAX: usize = ${contract.contextLayout.detailMax};
${rustStrSlice("steerWorkingVerbs", contract.steerWorking.verbs)}
pub const STEER_WORKING_TOKEN_TICK_MS: i64 = ${contract.steerWorking.tokenTickMs};
pub const STEER_WORKING_PREVIEW_MAX: usize = ${contract.steerWorking.previewMax};

pub const ISSUE_STATUS_STARTED_MAX: usize = ${contract.issueStatusCategory.startedMax};
pub const CODING_SESSION_STALE_MS: i64 = ${codingSessionStaleMs};
${rustStrSlice("codingSessionLaunchKeys", contract.codingSession.launchKeys)}
pub const CODING_SESSION_COMPUTER_USE_CAP: &str = "${contract.codingSession.computerUseCap}";
pub const DEVICE_ONLINE_WINDOW_MS: i64 = ${deviceOnlineWindowMs};
pub const SESSIONS_SEND_TEXT_MAX: usize = ${contract.sessionsSend.textMax};
pub const SESSIONS_SEND_MAX_ATTACHMENTS: usize = ${contract.sessionsSend.maxAttachments};
pub const SESSIONS_SEND_MAX_ISSUES: usize = ${contract.sessionsSend.maxIssues};
pub const SESSIONS_SEND_BRIEF_MAX_BYTES: usize = ${contract.sessionsSend.briefMaxBytes};
pub const SESSIONS_SEND_MESSAGE_BODY_MAX: usize = ${contract.sessionsSend.messageBodyMax};
pub const SESSIONS_SEND_AGENT_NAME_MAX: usize = ${contract.sessionsSend.agentNameMax};
pub const SESSIONS_SEND_AGENT_INSTRUCTIONS_MAX_BYTES: usize = ${contract.sessionsSend.agentInstructionsMaxBytes};
pub const SESSIONS_SEND_TREE_MAX_DEPTH: usize = ${contract.sessionsSend.treeMaxDepth};
pub const SESSIONS_SEND_TREE_MAX_SESSIONS: usize = ${contract.sessionsSend.treeMaxSessions};
pub const TEAM_AGENT_PROMPT_MAX_BYTES: usize = ${contract.team.agentPromptMaxBytes};
pub const DEVICE_AGENT_DEFAULTS_MODEL: &str = "${contract.deviceAgentDefaults.model}";
pub const DEVICE_AGENT_DEFAULTS_SUBAGENT_MODEL: &str = "${contract.deviceAgentDefaults.subagentModel}";
pub const DEVICE_AGENT_DEFAULTS_THREAD_MODEL: &str = "${contract.deviceAgentDefaults.threadModel}";
pub const DEVICE_AGENT_DEFAULTS_CODEX_MODEL: &str = "${contract.deviceAgentDefaults.codexModel}";
pub const DEVICE_AGENT_DEFAULTS_CODEX_THREAD_MODEL: &str = "${contract.deviceAgentDefaults.codexThreadModel}";
pub const DEVICE_COMPUTER_USE_DEFAULTS_MODEL: &str = "${contract.deviceComputerUseDefaults.model}";
pub const ACTION_INPUTS_MAX: usize = ${contract.actionInputs.max};
pub const ACTION_INPUT_TEXT_MAX: usize = ${contract.actionInputs.maxTextLength};
pub const START_PROMPT_MAX_LENGTH: usize = ${contract.startPrompt.maxLength};
pub const START_PROMPT_MAX_IMAGES: usize = ${contract.startPrompt.maxImages};
pub const AGENT_TRIGGER_MAX_PER_AGENT: usize = ${contract.agentTrigger.maxPerAgent};
pub const AGENT_TRIGGER_CRON_MAX: usize = ${contract.agentTrigger.cronMax};
pub const AGENT_TRIGGER_TZ_MAX: usize = ${contract.agentTrigger.tzMax};
pub const AGENT_TRIGGER_CATCH_UP_HOURS: i64 = ${contract.agentTrigger.catchUpHours};
pub const EVENT_NAME_MAX: usize = ${contract.events.nameMax};
pub const EVENT_PAYLOAD_MAX_BYTES: usize = ${contract.events.payloadMaxBytes};
pub const EVENT_RETENTION_DAYS: i64 = ${contract.events.retentionDays};
pub const STEER_FEED_BYTE_CAP: usize = ${steerFeed.byteCap};
pub const STEER_FEED_ITEM_CAP: usize = ${steerFeed.itemCap};
pub const STEER_FEED_TRIM_TARGET_PERCENT: usize = ${steerFeed.trimTargetPercent};
pub const STEER_FEED_ITEM_OVERHEAD_BYTES: usize = ${steerFeed.itemOverheadBytes};
pub const STEER_FEED_WINDOW: usize = ${steerFeed.window};
pub const STEER_FEED_WINDOW_STEP: usize = ${steerFeed.windowStep};
pub const STEER_FEED_HISTORY_PAGE_MAX: u32 = ${steerFeed.historyPageMax};
pub const STEER_FEED_TOOL_DIFF_MAX_LINES: usize = ${steerFeed.toolDiffMaxLines};
pub const STEER_FEED_TOOL_DIFF_MAX_BYTES: usize = ${steerFeed.toolDiffMaxBytes};
pub const STEER_FEED_TOOL_OUTPUT_MAX_LINES: usize = ${steerFeed.toolOutputMaxLines};
pub const STEER_FEED_TOOL_OUTPUT_MAX_BYTES: usize = ${steerFeed.toolOutputMaxBytes};
${rustDiffUi}
${rustComposerUi}

${rustNamedValues("issueStatusCategory", contract.issueStatusCategory.values)}
${rustNamedValues("issueSource", contract.issueSource.values)}
${rustNamedValues("issueEstimation", contract.issueEstimation.values)}
${rustNamedValues("teamRole", contract.teamRole.values)}
${rustNamedValues("commentKind", contract.commentKind.values)}
${rustNamedValues("commentSource", contract.commentSource.values)}
${rustNamedValues("commentAudience", contract.commentAudience.values)}
${rustNamedValues("notificationType", contract.notificationType.values)}
${rustNamedValues("prState", contract.prState.values)}
${rustNamedValues("sessionKind", contract.sessionKind.values)}
${rustNamedValues("sessionStatus", contract.sessionStatus.values)}
${rustNamedValues("subscriberSource", contract.subscriberSource.values)}
${rustNamedValues("pinKind", contract.pinKind.values)}
${rustNamedValues("issueEventType", contract.issueEventType.values)}
${rustNamedValues("issueRelationType", contract.issueRelationType.values)}
${rustNamedValues("issueRelationSource", contract.issueRelationSource.values)}
${rustNamedValues("sessionIssueSource", contract.sessionIssueSource.values)}
${rustNamedValues("messageAuthorKind", contract.messageAuthorKind.values)}
${rustNamedValues("conversationKind", contract.conversationKind.values)}
${rustNamedValues("messageCardKind", contract.messageCardKind.values)}
${rustNamedValues("messageEventKind", contract.messageEventKind.values)}
`

const swiftPath = join(
  repoRoot,
  "apps/ios/ExpCore/Sources/Domain/DomainContract.generated.swift"
)
const kotlinPath = join(
  repoRoot,
  "apps/android/app/src/main/java/com/exponential/app/domain/DomainContract.generated.kt"
)
const rustPath = join(
  repoRoot,
  "apps/desktop/crates/domain/src/contract.generated.rs"
)

mkdirSync(dirname(swiftPath), { recursive: true })
mkdirSync(dirname(kotlinPath), { recursive: true })
mkdirSync(dirname(rustPath), { recursive: true })
writeFileSync(swiftPath, swift)
writeFileSync(kotlinPath, kotlin)
writeFileSync(rustPath, rust)

console.log(`Wrote ${swiftPath}`)
console.log(`Wrote ${kotlinPath}`)
console.log(`Wrote ${rustPath}`)
