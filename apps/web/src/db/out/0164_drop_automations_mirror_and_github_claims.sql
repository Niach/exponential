-- SLOP-17: the legacy `automations` mirror of `actions.triggers` is gone;
-- `coding_sessions.automation_id` keeps holding the firing trigger's id, with
-- no FK. The constraint drops FIRST: DROP TABLE … CASCADE would take it along
-- and a later explicit drop would fail.
ALTER TABLE "coding_sessions" DROP CONSTRAINT IF EXISTS "coding_sessions_automation_id_automations_id_fk";--> statement-breakpoint
DROP TABLE "automations" CASCADE;--> statement-breakpoint
-- SLOP-24: the retired GitHub claim tables (SLOP-7) and the installation
-- webhook mirror nothing read.
DROP TABLE "github_installation_links" CASCADE;--> statement-breakpoint
DROP TABLE "github_installation_repo_grants" CASCADE;--> statement-breakpoint
DROP TABLE "github_installations" CASCADE;--> statement-breakpoint
DROP TABLE "github_user_identities" CASCADE;
