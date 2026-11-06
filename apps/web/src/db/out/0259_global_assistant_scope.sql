ALTER TABLE "messages" ADD COLUMN "scope_team_ids" uuid[];--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "scope_team_ids" uuid[];--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "kind" varchar(16) DEFAULT 'team' NOT NULL;--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "owner_id" text;--> statement-breakpoint
ALTER TABLE "teams" ADD CONSTRAINT "teams_owner_id_users_id_fk" FOREIGN KEY ("owner_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_teams_personal_owner" ON "teams" USING btree ("owner_id") WHERE kind = 'personal';