CREATE TABLE "import_id_map" (
	"import_id" uuid NOT NULL,
	"table_name" varchar(32) NOT NULL,
	"local_id" text NOT NULL,
	"server_id" text NOT NULL,
	CONSTRAINT "import_id_map_import_id_table_name_local_id_pk" PRIMARY KEY("import_id","table_name","local_id")
);
--> statement-breakpoint
CREATE TABLE "import_rows" (
	"import_id" uuid NOT NULL,
	"seq" integer NOT NULL,
	"ordinal" integer NOT NULL,
	"table_name" varchar(32) NOT NULL,
	"local_id" text NOT NULL,
	"row" jsonb NOT NULL,
	CONSTRAINT "import_rows_import_id_seq_ordinal_pk" PRIMARY KEY("import_id","seq","ordinal")
);
--> statement-breakpoint
CREATE TABLE "imports" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"created_by_user_id" text NOT NULL,
	"status" varchar(16) DEFAULT 'importing' NOT NULL,
	"manifest" jsonb NOT NULL,
	"counts" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"error" text,
	"committed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "imported_from" jsonb;--> statement-breakpoint
ALTER TABLE "import_id_map" ADD CONSTRAINT "import_id_map_import_id_imports_id_fk" FOREIGN KEY ("import_id") REFERENCES "public"."imports"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "import_rows" ADD CONSTRAINT "import_rows_import_id_imports_id_fk" FOREIGN KEY ("import_id") REFERENCES "public"."imports"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "imports" ADD CONSTRAINT "imports_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "imports" ADD CONSTRAINT "imports_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_import_rows_table" ON "import_rows" USING btree ("import_id","table_name");--> statement-breakpoint
CREATE INDEX "idx_imports_team" ON "imports" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_imports_live" ON "imports" USING btree ("status","created_at") WHERE status in ('importing', 'failed');