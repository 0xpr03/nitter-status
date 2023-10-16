use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd = r#"CREATE TABLE "check_errors" (
            "time" integer NOT NULL,
            "host" integer NOT NULL,
            "message" text NOT NULL,
            "http_body" text,
            "http_status" integer,
            CONSTRAINT "pk_check_errors" PRIMARY KEY ("host", "time"),
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) WITHOUT ROWID, STRICT;"#;
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        tracing::info!("adding check_errors table..");
        db.execute_unprepared(cmd).await?;
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
