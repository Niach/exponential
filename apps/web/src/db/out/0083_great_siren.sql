CREATE TABLE "github_user_identities" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" text NOT NULL,
	"github_user_id" bigint NOT NULL,
	"github_login" text NOT NULL,
	"verified_at" timestamp with time zone DEFAULT now() NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "uniq_github_user_identities_github_user_id" UNIQUE("github_user_id")
);
--> statement-breakpoint
ALTER TABLE "github_user_identities" ADD CONSTRAINT "github_user_identities_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_github_user_identities_user" ON "github_user_identities" USING btree ("user_id");