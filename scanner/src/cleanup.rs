use entities::check_errors;
use entities::health_check;
use entities::host;
use sea_orm::ColumnTrait;
use sea_orm::EntityTrait;
use sea_orm::Order;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::QuerySelect;
use sea_query::Query;
use tokio::time::interval;

use crate::Scanner;
use crate::Result;

impl Scanner {
    /// Setup scheduled job for cleaning up old data
    pub(crate) fn schedule_cleanup(&self) -> Result<()> {
        let c = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(c.inner.config.cleanup_interval);
            loop {
                interval.tick().await;
                if let Err(e) = c.cleanup().await {
                    tracing::error!(error=%e);
                }
            }
        });
        Ok(())
    }
    /// Perform cleanup of outdated data
    async fn cleanup(&self) -> Result<()> {
        self.cleanup_errors().await?;
        Ok(())
    }

    /// Remove all but recent host error entries
    async fn cleanup_errors(&self) -> Result<()> {
        // I wish this was easier without row ids or giant SQL queries
        let hosts = host::Entity::find().all(&self.inner.db).await?;
        for host in hosts {
            let res = check_errors::Entity::delete_many()
            .filter(check_errors::Column::Host.eq(host.id))
            .filter(check_errors::Column::Time.not_in_subquery(
                Query::select()
                .column(check_errors::Column::Time)
                .from(check_errors::Entity)
                .and_where(check_errors::Column::Host.eq(host.id))
                .order_by(check_errors::Column::Time, Order::Desc)
                .limit(self.inner.config.error_retention_per_host as _)
                .to_owned()
            ))
            .exec(&self.inner.db).await?;
            tracing::debug!(host=host.id,deleted_errors=res.rows_affected);
        }

        Ok(())
    }
}