use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd_mail = r#"CREATE TABLE "instance_mail" (
            "host" integer NOT NULL PRIMARY KEY,
            "mail" text NOT NULL,
            "verified" integer NOT NULL,
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) WITHOUT ROWID, STRICT;"#;
        let cmd_alerts = r#"CREATE TABLE "instance_alerts" (
            "host" integer NOT NULL PRIMARY KEY,
            "host_down_retries" integer,
            "avg_account_age_days" integer,
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) WITHOUT ROWID, STRICT;"#;
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        tracing::info!("adding instance_mail table..");
        db.execute_unprepared(cmd_mail).await?;
        tracing::info!("adding instance_alerts table..");
        db.execute_unprepared(cmd_alerts).await?;
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
