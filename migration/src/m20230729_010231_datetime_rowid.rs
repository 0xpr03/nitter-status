use std::borrow::Cow;

use sea_orm_migration::{
    prelude::*,
    sea_orm::{
        prelude::DateTimeUtc, DbBackend, DeriveEntityModel, FromQueryResult, RuntimeErr, Statement,
    },
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Debug, FromQueryResult)]
pub struct UpdateCheck {
    time: DateTimeUtc,
    host: i32,
    resp_time: Option<i32>,
    healthy: bool,
    response_code: Option<i32>,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd = r#"CREATE TABLE "health_check_new" (
            "time" integer NOT NULL,
            "host" integer NOT NULL,
            "resp_time" integer,
            "healthy" integer NOT NULL,
            "response_code" integer,
            CONSTRAINT "pk_health_check_new" PRIMARY KEY ("host", "time"),
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) WITHOUT ROWID, STRICT;"#;
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        db.execute_unprepared(cmd).await?;
        tracing::info!("fetching all healtcheck entries..");
        let data = UpdateCheck::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"SELECT * FROM update_check"#,
            [],
        ))
        .all(db)
        .await?;

        tracing::info!("migrating {} entries to temp table..", data.len());
        let mut duplicates = 0;
        for entry in data.into_iter() {
            if let Err(err) = db
                .execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    r#"INSERT INTO health_check_new (
                    time,
                    host,
                    resp_time,
                    healthy,
                    response_code) VALUES ($1,$2,$3,$4,$5)"#,
                    [
                        entry.time.timestamp().into(),
                        entry.host.into(),
                        entry.resp_time.into(),
                        entry.healthy.into(),
                        entry.response_code.into(),
                    ],
                ))
                .await
            {
                if let DbErr::Exec(RuntimeErr::SqlxError(e)) = &err {
                    if let Some(err) = e.as_database_error() {
                        if err.code() == Some(Cow::Borrowed("1555")) {
                            duplicates += 1;
                            continue;
                        }
                    }
                }
                tracing::info!(entry=?entry);
                return Err(err);
            }
        }
        tracing::info!("dropped {} duplicate timestamps..", duplicates);
        tracing::info!("dropping old table..");
        db.execute_unprepared("DROP TABLE update_check").await?;

        db.execute_unprepared(&cmd.replace("health_check_new", "health_check"))
            .await?;
        tracing::info!("inserting back into final table..");
        db.execute_unprepared(
            r#"INSERT INTO health_check
            SELECT * 
            FROM health_check_new WHERE true"#,
        )
        .await?;
        tracing::info!("dropping temp table..");
        db.execute_unprepared("DROP TABLE health_check_new").await?;
        tracing::info!("cleaning up db..");
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
