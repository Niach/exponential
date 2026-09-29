-- EXP-1132 sweep: team deletes used to cascade a placeholder member's seat
-- but never its users row, leaving zero-team ghosts (an import's roster whose
-- team was deleted). A later invite to such an address came out UNBOUND, so
-- accepting it from another account never merged the ghost away. Team
-- deletes purge them now (lib/placeholder-members.ts
-- `deletePlaceholdersIfOrphaned`); this drops the ones already stranded, with
-- the exact `deletePlaceholderIfOrphaned` predicate. Idempotent.
DELETE FROM "users" u
WHERE u."placeholder_at" IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM "team_members" tm WHERE tm."user_id" = u."id")
  AND NOT EXISTS (SELECT 1 FROM "issues" i WHERE i."assignee_id" = u."id" OR i."creator_id" = u."id")
  AND NOT EXISTS (SELECT 1 FROM "comments" c WHERE c."author_id" = u."id")
  AND NOT EXISTS (SELECT 1 FROM "issue_events" e WHERE e."actor_user_id" = u."id")
  AND NOT EXISTS (SELECT 1 FROM "attachments" a WHERE a."uploader_id" = u."id")
  AND NOT EXISTS (SELECT 1 FROM "workflows" w WHERE w."creator_id" = u."id");
