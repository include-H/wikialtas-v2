//! sqlx 仓储层。
//!
//! 这条纪律来自 `docs/database.md` §0.3：**一切多步写在一个事务里**。
//! v1 的 `CreateDoc` 是三次裸 Exec、`PatchWork` 是七条独立 UPDATE，崩在中间就半截。

pub mod content;
pub mod jobs;
pub mod library;
pub mod nodes;
pub mod secrets;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub use wikiatlas_domain as domain;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Domain(#[from] domain::Error),
    #[error("数据库错误：{0}")]
    Db(#[from] sqlx::Error),
    #[error("迁移失败：{0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 建连接池。**不**自动迁移（迁移显式调一次，免得每个测试都在抢).
pub async fn connect(url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new().max_connections(10).connect(url).await?;
    Ok(pool)
}

/// 跑迁移。文件名是 sqlx 约定（`migrations/NNNN_说明.sql`），路径相对本 crate。
pub async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// 把「唯一键冲突」翻译成领域错误——不要让 23505 原样漏到 API 层。
pub(crate) fn map_unique(e: sqlx::Error, what: &str) -> Error {
    if let sqlx::Error::Database(ref db) = e {
        if db.code().as_deref() == Some("23505") {
            return Error::Domain(domain::Error::Duplicated(what.to_string()));
        }
    }
    Error::Db(e)
}
