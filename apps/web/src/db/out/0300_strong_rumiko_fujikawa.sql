CREATE TABLE "steer_relay_boots" (
	"boot" varchar(64) PRIMARY KEY NOT NULL,
	"started_at" timestamp with time zone NOT NULL,
	"seen_at" timestamp with time zone NOT NULL,
	"counters" jsonb NOT NULL
);
--> statement-breakpoint
CREATE TABLE "voice_calls" (
	"call_id" uuid PRIMARY KEY NOT NULL,
	"tag" varchar(8) NOT NULL,
	"user_id" text NOT NULL,
	"team_id" uuid NOT NULL,
	"agent_id" uuid,
	"device_id" varchar(128) NOT NULL,
	"platform" varchar(16) NOT NULL,
	"app_version" varchar(64),
	"mode" varchar(8) DEFAULT 'call' NOT NULL,
	"sealed" boolean NOT NULL,
	"minted_at" timestamp with time zone NOT NULL,
	"host_ticket_at" timestamp with time zone,
	"answered_ms" integer,
	"host_ended_by" varchar(16),
	"host_reason" varchar(64),
	"relay_boot" varchar(64),
	"rang_at" timestamp with time zone,
	"paired_at" timestamp with time zone,
	"first_down_at" timestamp with time zone,
	"last_up_at" timestamp with time zone,
	"last_down_at" timestamp with time zone,
	"frames_up" integer DEFAULT 0 NOT NULL,
	"frames_down" integer DEFAULT 0 NOT NULL,
	"frames_dropped" integer DEFAULT 0 NOT NULL,
	"frames_skipped" integer DEFAULT 0 NOT NULL,
	"ended_at" timestamp with time zone,
	"ended_by" varchar(16),
	"reason" varchar(64)
);
--> statement-breakpoint
ALTER TABLE "voice_calls" ADD CONSTRAINT "voice_calls_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "voice_calls" ADD CONSTRAINT "voice_calls_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "voice_calls" ADD CONSTRAINT "voice_calls_agent_id_agents_id_fk" FOREIGN KEY ("agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_steer_relay_boots_seen" ON "steer_relay_boots" USING btree ("seen_at");--> statement-breakpoint
CREATE INDEX "idx_voice_calls_tag" ON "voice_calls" USING btree ("tag");--> statement-breakpoint
CREATE INDEX "idx_voice_calls_minted" ON "voice_calls" USING btree ("minted_at");--> statement-breakpoint
CREATE INDEX "idx_voice_calls_team_minted" ON "voice_calls" USING btree ("team_id","minted_at");--> statement-breakpoint
CREATE INDEX "idx_voice_calls_user" ON "voice_calls" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_voice_calls_open" ON "voice_calls" USING btree ("minted_at") WHERE ended_at IS NULL;