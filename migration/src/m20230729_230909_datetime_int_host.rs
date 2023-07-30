use sea_orm_migration::{
    prelude::*,
    sea_orm::{prelude::DateTimeUtc, DbBackend, FromQueryResult, Statement},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Debug, FromQueryResult)]
pub struct Host {
    pub id: i32,
    pub domain: String,
    pub url: String,
    pub enabled: bool,
    pub rss: bool,
    pub version: Option<String>,
    pub updated: DateTimeUtc,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd = r#"CREATE TABLE "host_new" (
            "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
            "domain" text NOT NULL UNIQUE,
            "url" text NOT NULL,
            "version" text,
            "enabled" integer NOT NULL,
            "rss" integer NOT NULL,
            "updated" integer NOT NULL
        ) STRICT;"#;
        let db = manager.get_connection();
        db.execute_unprepared("PRAGMA foreign_keys = 0").await?;
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        db.execute_unprepared(cmd).await?;
        tracing::info!("fetching all host entries..");
        let data = Host::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"SELECT * FROM host"#,
            [],
        ))
        .all(db)
        .await?;

        tracing::info!("migrating {} entries to temp table..", data.len());
        for entry in data.into_iter() {
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                r#"INSERT INTO host_new (
                    id,
                    domain,
                    url,
                    version,
                    enabled,
                    rss,
                    updated ) VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
                [
                    entry.id.into(),
                    entry.domain.into(),
                    entry.url.into(),
                    entry.version.into(),
                    entry.enabled.into(),
                    entry.rss.into(),
                    entry.updated.timestamp().into(),
                ],
            ))
            .await?;
        }
        tracing::info!("dropping old table..");
        db.execute_unprepared("DROP TABLE host").await?;

        db.execute_unprepared(&cmd.replace("host_new", "host"))
            .await?;
        tracing::info!("inserting back into final table..");
        db.execute_unprepared(
            r#"INSERT INTO host
            SELECT * 
            FROM host_new WHERE true"#,
        )
        .await?;
        tracing::info!("dropping temp table..");
        db.execute_unprepared("DROP TABLE host_new").await?;
        tracing::info!("cleaning up db..");
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("PRAGMA foreign_keys = 1").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Post {
    Table,
    Id,
    Title,
    Text,
}
