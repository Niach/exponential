ALTER TABLE "agent_registrations" DROP CONSTRAINT "agent_registrations_workspace_id_user_id_unique";--> statement-breakpoint
ALTER TABLE "agent_registrations" DROP CONSTRAINT "agent_registrations_workspace_id_workspaces_id_fk";
--> statement-breakpoint
DROP INDEX "idx_agent_registrations_workspace";--> statement-breakpoint
ALTER TABLE "agent_registrations" ADD COLUMN "device_id" text NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_agent_registrations_device" ON "agent_registrations" USING btree ("device_id");--> statement-breakpoint
ALTER TABLE "agent_registrations" DROP COLUMN "workspace_id";--> statement-breakpoint
ALTER TABLE "agent_registrations" DROP COLUMN "oauth_client_id";--> statement-breakpoint
ALTER TABLE "agent_registrations" ADD CONSTRAINT "agent_registrations_owner_user_id_device_id_unique" UNIQUE("owner_user_id","device_id");