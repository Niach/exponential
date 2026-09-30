ALTER TABLE "automations" DROP CONSTRAINT "automations_action_id_actions_id_fk";
--> statement-breakpoint
ALTER TABLE "automations" ALTER COLUMN "action_id" SET DATA TYPE text;