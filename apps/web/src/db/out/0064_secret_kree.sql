ALTER TABLE "coding_sessions" ADD COLUMN "host_user_id" text;--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "shared_team_id" uuid;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD CONSTRAINT "coding_sessions_host_user_id_users_id_fk" FOREIGN KEY ("host_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "devices" ADD CONSTRAINT "devices_shared_team_id_teams_id_fk" FOREIGN KEY ("shared_team_id") REFERENCES "public"."teams"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_devices_shared_team" ON "devices" USING btree ("shared_team_id") WHERE shared_team_id IS NOT NULL;