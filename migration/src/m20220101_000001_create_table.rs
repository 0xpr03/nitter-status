// SPDX-License-Identifier: AGPL-3.0-only
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Host::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Host::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Host::Domain)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Host::URL).string().not_null())
                    .col(ColumnDef::new(Host::Version).string())
                    .col(ColumnDef::new(Host::Enabled).boolean().not_null())
                    .col(ColumnDef::new(Host::RSS).boolean().not_null())
                    .col(ColumnDef::new(Host::Updated).date_time().not_null())
                    .to_owned(),
            )
            .await
            .unwrap();
        manager
            .create_table(
                Table::create()
                    .table(UpdateCheck::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(UpdateCheck::Time).date_time().not_null())
                    .col(ColumnDef::new(UpdateCheck::Host).integer().not_null())
                    .col(ColumnDef::new(UpdateCheck::RespTime).integer())
                    .col(ColumnDef::new(UpdateCheck::Healthy).boolean().not_null())
                    .col(ColumnDef::new(UpdateCheck::ResponseCode).integer())
                    .foreign_key(
                        ForeignKey::create()
                            .name("FK_updatecheck_host")
                            .from(UpdateCheck::Table, UpdateCheck::Host)
                            .to(Host::Table, Host::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .primary_key(
                        Index::create()
                            .col(UpdateCheck::Host)
                            .col(UpdateCheck::Time)
                            .name("pk_updatecheck"),
                    )
                    .to_owned(),
            )
            .await
            .unwrap();
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(UpdateCheck::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
            .unwrap();
        manager
            .drop_table(Table::drop().table(Host::Table).if_exists().to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Host {
    Table,
    Id,
    Domain,
    URL,
    RSS,
    Version,
    Enabled,
    Updated,
}

#[derive(Iden)]
enum UpdateCheck {
    Table,
    Host,
    Time,
    Healthy,
    RespTime,
    ResponseCode,
}
