use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd_mail = r#"CREATE TABLE "instance_mail" (
            "host" integer NOT NULL PRIMARY KEY,
            "mail" text NOT NULL,
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) WITHOUT ROWID, STRICT;"#;
        let cmd_alerts = r#"CREATE TABLE "instance_alerts" (
            "host" integer NOT NULL PRIMARY KEY,
            "host_down_amount" integer,
            "host_down_amount_enable" integer,
            "alive_accs_min_threshold" integer,
            "alive_accs_min_threshold_enable" integer,
            "alive_accs_min_percent" integer,
            "alive_accs_min_percent_enable" integer,
            "avg_account_age_days" integer,
            "avg_account_age_days_enable" integer,
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) WITHOUT ROWID, STRICT;"#;
        let cmd_verification = r#"CREATE TABLE "mail_verification_tokens" (
            "host" integer NOT NULL PRIMARY KEY,
            "public_part" text NOT NULL UNIQUE,
            "secret_part" text NOT NULL,
            "mail" text NOT NULL,
            "eol_date" integer NOT NULL,
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) WITHOUT ROWID, STRICT;"#;
        let cmd_last_mail = r#"CREATE TABLE "last_mail_send" (
            "mail" text NOT NULL,
            "kind" integer NOT NULL,
            "time" integer NOT NULL,
            PRIMARY KEY("mail","kind")
        ) WITHOUT ROWID, STRICT;"#;
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        tracing::info!("adding instance_mail table..");
        db.execute_unprepared(cmd_mail).await?;
        tracing::info!("adding instance_alerts table..");
        db.execute_unprepared(cmd_alerts).await?;
        tracing::info!("adding verification_tokens table..");
        db.execute_unprepared(cmd_verification).await?;
        tracing::info!("adding last_mail_send table..");
        db.execute_unprepared(cmd_last_mail).await?;
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
