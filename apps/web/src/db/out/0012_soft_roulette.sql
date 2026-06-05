-- Drain the legacy agent plan/question comments. Their content was migrated into
-- issue_agent_state by 0010 (and the server no longer dual-writes them), so these
-- rows are dead — every client now renders plan/question from the structured Plan
-- Panel. attachments.comment_id is ON DELETE SET NULL, so this is safe. Run before
-- the CHECK constraint below, which would otherwise reject the existing rows.
DELETE FROM "comments" WHERE "kind" <> 'regular';
--> statement-breakpoint
ALTER TABLE "comments" ADD CONSTRAINT "comments_kind_regular" CHECK ("comments"."kind" = 'regular');
