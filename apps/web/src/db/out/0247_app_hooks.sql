CREATE TABLE "app_hook_nonces" (
	"hook_id" uuid NOT NULL,
	"nonce" varchar(64) NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "app_hook_nonces_hook_id_nonce_pk" PRIMARY KEY("hook_id","nonce")
);
--> statement-breakpoint
CREATE TABLE "app_hooks" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"app_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"name" varchar(40) NOT NULL,
	"secret_ciphertext" text NOT NULL,
	"rotated_by" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "app_hooks_app_name_unique" UNIQUE("app_id","name")
);
--> statement-breakpoint
ALTER TABLE "app_hook_nonces" ADD CONSTRAINT "app_hook_nonces_hook_id_app_hooks_id_fk" FOREIGN KEY ("hook_id") REFERENCES "public"."app_hooks"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_hooks" ADD CONSTRAINT "app_hooks_app_id_apps_id_fk" FOREIGN KEY ("app_id") REFERENCES "public"."apps"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_hooks" ADD CONSTRAINT "app_hooks_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_hooks" ADD CONSTRAINT "app_hooks_rotated_by_users_id_fk" FOREIGN KEY ("rotated_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_app_hook_nonces_created" ON "app_hook_nonces" USING btree ("created_at");