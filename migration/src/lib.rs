pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20230729_010231_datetime_rowid;
mod m20230729_230909_datetime_int_host;
mod m20230803_154714_version_url;
mod m20230829_201916_country;
mod m20230914_231514_connectivity;
mod m20231011_231223_errors;
mod m20231112_142206_stats;
mod m20231129_003005_mail;


pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20230729_010231_datetime_rowid::Migration),
            Box::new(m20230729_230909_datetime_int_host::Migration),
            Box::new(m20230803_154714_version_url::Migration),
            Box::new(m20230829_201916_country::Migration),
            Box::new(m20230914_231514_connectivity::Migration),
            Box::new(m20231011_231223_errors::Migration),
            Box::new(m20231112_142206_stats::Migration),
            Box::new(m20231129_003005_mail::Migration),
        ]
    }
}
