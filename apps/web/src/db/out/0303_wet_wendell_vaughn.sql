ALTER TABLE "devices" ADD COLUMN "assistant_team_ids" uuid[] DEFAULT '{}'::uuid[] NOT NULL;--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "space_host_id" uuid;--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "space_team_id" uuid;--> statement-breakpoint
ALTER TABLE "devices" ADD CONSTRAINT "devices_space_host_id_devices_id_fk" FOREIGN KEY ("space_host_id") REFERENCES "public"."devices"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "devices" ADD CONSTRAINT "devices_space_team_id_teams_id_fk" FOREIGN KEY ("space_team_id") REFERENCES "public"."teams"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_devices_space_user" ON "devices" USING btree ("user_id") WHERE "devices"."space_host_id" IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_devices_space_host" ON "devices" USING btree ("space_host_id") WHERE "devices"."space_host_id" IS NOT NULL;