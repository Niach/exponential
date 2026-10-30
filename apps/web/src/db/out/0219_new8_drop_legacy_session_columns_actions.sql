-- NEW-8 step 4: the server drop. Every client at the floor (iOS 0.14.71,
-- Android 0.14.73, desktop + CLI 0.14.83) reads none of this: the legacy
-- session columns and `session_issues.assigned_session_id` rode no shape,
-- the `actions` shape is gone and `pins.action_id` leaves the pins shape in
-- the same deploy. `action` pins never resolve on any client: delete them
-- before the pin kind loses the value. FKs first, then the table.
DELETE FROM "pins" WHERE "kind" = 'action';--> statement-breakpoint
ALTER TABLE "pins" DROP CONSTRAINT "pins_kind_target_check";--> statement-breakpoint
ALTER TABLE "pins" DROP CONSTRAINT "pins_action_id_actions_id_fk";
--> statement-breakpoint
ALTER TABLE "session_issues" DROP CONSTRAINT "session_issues_assigned_session_id_sessions_id_fk";
--> statement-breakpoint
ALTER TABLE "sessions" DROP CONSTRAINT "sessions_action_id_actions_id_fk";
--> statement-breakpoint
DROP TABLE "actions";--> statement-breakpoint
ALTER TABLE "pins" ALTER COLUMN "kind" SET DATA TYPE text;--> statement-breakpoint
DROP TYPE "public"."pin_kind";--> statement-breakpoint
CREATE TYPE "public"."pin_kind" AS ENUM('issue', 'session');--> statement-breakpoint
ALTER TABLE "pins" ALTER COLUMN "kind" SET DATA TYPE "public"."pin_kind" USING "kind"::"public"."pin_kind";--> statement-breakpoint
DROP INDEX "uniq_pins_user_action";--> statement-breakpoint
DROP INDEX "idx_pins_action";--> statement-breakpoint
DROP INDEX "idx_sessions_action";--> statement-breakpoint
ALTER TABLE "pins" DROP COLUMN "action_id";--> statement-breakpoint
ALTER TABLE "session_issues" DROP COLUMN "assigned_session_id";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "kind";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "dispatch";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "move_to_device_id";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "handoff_restored_at";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "caption";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "aside";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "action_id";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "action_name";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "action_inputs";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "spawned_by_session_id";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "name";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "icon";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "color";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "triggers";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "sort_order";--> statement-breakpoint
ALTER TABLE "pins" ADD CONSTRAINT "pins_kind_target_check" CHECK ((kind = 'issue' AND issue_id IS NOT NULL AND session_id IS NULL) OR (kind = 'session' AND session_id IS NOT NULL AND issue_id IS NULL));--> statement-breakpoint
DROP TYPE "public"."session_kind";