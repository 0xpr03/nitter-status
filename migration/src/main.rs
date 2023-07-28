// SPDX-License-Identifier: AGPL-3.0-only
use sea_orm_migration::prelude::*;

#[async_std::main]
async fn main() {
    cli::run_cli(migration::Migrator).await;
}
