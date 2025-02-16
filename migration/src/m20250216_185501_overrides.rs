use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd_host_overrides = r#"CREATE TABLE "host_overrides" (
            "host" integer NOT NULL,
            "key" text NOT NULL,
            "locked" integer NOT NULL,
            "value" text,
            CONSTRAINT "pk_override_hostkey" PRIMARY KEY ("host", "key"),
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) STRICT;"#;
        let cmd_overrides_index = r#"CREATE INDEX "index_overrides_key" ON host_overrides ("key");"#;
        let cmd_log = r#"CREATE TABLE "log" (
            "user_host" integer NOT NULL,
            "host_affected" integer,
            "key" text NOT NULL,
            "time" integer NOT NULL,
            "new_value" text,
            CONSTRAINT "pk_log_usertime" PRIMARY KEY ("user_host", "time"),
            FOREIGN KEY ("host_affected") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
            FOREIGN KEY ("user_host") REFERENCES "host" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
        ) STRICT;"#;
        let cmd_log_index = r#"CREATE INDEX "index_log_time" ON log ("time");"#;
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        tracing::info!("adding host overrides table..");
        db.execute_unprepared(cmd_host_overrides).await?;
        tracing::info!("adding host overrides index..");
        db.execute_unprepared(cmd_overrides_index).await?;
        tracing::info!("adding log table..");
        db.execute_unprepared(cmd_log).await?;
        tracing::info!("adding log index..");
        db.execute_unprepared(cmd_log_index).await?;
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
