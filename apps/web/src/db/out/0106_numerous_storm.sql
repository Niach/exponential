ALTER TYPE "public"."notification_type" ADD VALUE 'agent_message';--> statement-breakpoint
ALTER TABLE "user_notification_prefs" ADD COLUMN "allow_agent_messages" boolean DEFAULT true NOT NULL;