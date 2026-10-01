ALTER TABLE "actions" ADD COLUMN "triggers" jsonb DEFAULT '[]'::jsonb NOT NULL;--> statement-breakpoint
-- SLOP-2: automations fold into their action as `actions.triggers`. Hand-
-- written from here on (precedent: 0082, which split them out).
--
-- 1. The tidy-up builtin has no row to carry triggers: a team that automates
--    it gets a REAL "Tidy up" action (the shipped program as its body, the
--    builtin's inputs) and its automations are repointed at it.
INSERT INTO "actions" ("team_id", "name", "description", "icon", "body", "inputs", "prompt_placeholder", "sort_order")
SELECT
  t.team_id,
  'Tidy up',
  'Let your agent dedupe, label and link a board''s issues. Nothing is deleted',
  'brush-cleaning',
  'Team: `' || t.team_id || E'`\n\n' || $tidy$Tidy the team's issue boards without changing any issue: link duplicates, apply existing labels and record the relations between issues. Every write goes through the Exponential MCP tools. The repository, when one is attached, is READ-ONLY context for understanding issues — never edit, commit or push.

## Hard rules

- Never edit an issue's title, description, status, priority, assignee, estimate or due date — do not call `exponential_issues_update`. Never delete an issue, a label or a relation; never remove a label.
- Never create, rename or delete labels: only labels returned by `exponential_labels_list` are eligible.
- Allowed writes: `exponential_issue_labels_add`, `exponential_issue_relations_add`, and `exponential_issues_create` for ONE combined issue when two near-duplicates hold different information (step 4).
- Leave no comments except the one closing comment on a combined issue (step 4). Never comment that no label fits.

## 1. Vocabulary

`exponential_labels_list` for the team: the name and description of every label. An empty set means no labeling.

## 2. Scope

- The `board` input names the board; without it, every board from `exponential_boards_list`.
- `exponential_issues_list` per board with `limit: 200`, `sort: "createdAt"`, paging with `offset` on exactly 200 rows. Open work only (the default).
- Skip issues that carry a `prUrl` or sit in a started or in-review status: work in flight is not yours to reorganise.
- When a `## Trigger` section names issues, tidy THOSE against the rest of the scope instead of the whole board.
- Additional instructions narrow this scope; they never loosen the hard rules.

## 3. Read

`exponential_issues_get` for every candidate: title, description, comments, attachments, labels, relations. Fan out background Agents over slices of the list; each returns per issue a one-line gist, candidate labels, suspected duplicates, ordering dependencies and part-of relations, with evidence.

## 4. Duplicates

Two issues are duplicates only when they describe the SAME problem or request.

- Same information: `exponential_issue_relations_add({ issueId: <newer>, relatedIssueId: <older>, type: "duplicate" })` — the newer duplicates the older, which also moves the newer one to the Duplicate status.
- Near-duplicates (same problem, different details, screenshots, reporters or steps): `exponential_issues_create` ONE combined issue on the same board holding ALL information from both (plain text; name the sources as `#IDENT`), mark both as duplicates of it, and post one comment on the combined issue naming its sources. No information may be lost.
- Unsure: leave both open and link them `related`.

## 5. Labels

Apply every existing label that clearly fits the issue's content (type of work, area, platform), typically 1–3 per issue, one `exponential_issue_labels_add` call per issue. Nothing fits: no label, no comment.

## 6. Relations

- `blocks` when one issue must land before another can start (`issueId` blocks `relatedIssueId`).
- `parent` when one issue is clearly a piece of another (`issueId` is the parent).
- Prefer no relation over a speculative one. The server refuses cycles; do not retry a refused edge.

## 7. Report

Per board: candidates, duplicates linked, combined issues created, labels applied, relations added, as a compact table (identifier · action · target). List labels you found yourself wanting as suggestions; never create them.$tidy$,
  '[{"key":"board","label":"Board","type":"board","required":false},{"key":"repo","label":"Repository","type":"repo","required":false}]'::jsonb,
  'Anything the tidy-up should focus on or leave alone (optional)…',
  COALESCE((SELECT max(x.sort_order) FROM "actions" x WHERE x.team_id = t.team_id), 0) + 1
FROM (
  SELECT DISTINCT team_id FROM "automations"
  WHERE action_id = 'builtin:tidy-up'
    AND jsonb_typeof(trigger) = 'object'
    AND trigger->>'kind' IN ('schedule', 'event')
) t
ON CONFLICT ("team_id", "name") DO NOTHING;--> statement-breakpoint
UPDATE "automations" au
SET action_id = ac.id::text
FROM "actions" ac
WHERE au.action_id = 'builtin:tidy-up'
  AND ac.team_id = au.team_id
  AND ac.name = 'Tidy up';--> statement-breakpoint
-- 2. `automations` becomes a mirror of `actions.triggers`: drop what cannot
--    be mirrored (an unreadable trigger, a target that no longer exists).
DELETE FROM "automations" au
WHERE jsonb_typeof(au.trigger) <> 'object'
   OR au.trigger->>'kind' NOT IN ('schedule', 'event')
   OR NOT EXISTS (SELECT 1 FROM "actions" ac WHERE ac.id::text = au.action_id);--> statement-breakpoint
-- 3. Each automation becomes one trigger, KEEPING its id (run attribution and
--    the devices' firing state key on it); unset pins are omitted and an
--    event names its source.
UPDATE "actions" ac
SET triggers = folded.triggers
FROM (
  SELECT
    au.action_id,
    jsonb_agg(
      jsonb_strip_nulls(jsonb_build_object(
        'id', au.id,
        'enabled', au.enabled,
        'deviceId', au.device_id,
        'agent', au.agent,
        'account', au.account,
        'model', au.model,
        'effort', au.effort
      ))
      || au.trigger
      || CASE WHEN au.trigger->>'kind' = 'event'
           THEN '{"source":"exponential"}'::jsonb
           ELSE '{}'::jsonb
         END
      ORDER BY au.sort_order, au.created_at
    ) AS triggers
  FROM "automations" au
  GROUP BY au.action_id
) folded
WHERE ac.id::text = folded.action_id;
