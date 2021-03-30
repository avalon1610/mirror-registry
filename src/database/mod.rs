pub mod schema;

use crate::{auth, config::Config};
use anyhow::{Context, Result};
use diesel::{
    query_builder::{AstPass, Query, QueryFragment},
    query_dsl::LoadQuery,
    sql_types::BigInt,
    sqlite::Sqlite,
    Connection, QueryResult, RunQueryDsl, SqliteConnection,
};
use diesel_migrations::embed_migrations;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Database {
    pub connection: SqliteConnection,
}

embed_migrations!();

impl Database {
    pub async fn new(config: Arc<RwLock<Config>>) -> Result<Self> {
        let connection = Database::establish_connection(config).await?;

        // This will run the necessary migrations.
        embedded_migrations::run(&connection).context("database migration failed")?;
        auth::setup_root(&connection)
            .await
            .context("setup super admin failed")?;
        Ok(Database { connection })
    }

    async fn establish_connection(config: Arc<RwLock<Config>>) -> Result<SqliteConnection> {
        Ok(
            SqliteConnection::establish(&config.read().await.database.url)
                .context("connect to database failed")?,
        )
    }
}

const DEFAULT_PER_PAGE: i64 = 10;

pub trait Paginate: Sized {
    fn paginate(self, page: i64) -> Paginated<Self>;
}

impl<T> Paginate for T {
    fn paginate(self, page: i64) -> Paginated<Self> {
        Paginated {
            query: self,
            per_page: DEFAULT_PER_PAGE,
            page,
        }
    }
}

#[derive(Debug, Clone, Copy, QueryId)]
pub struct Paginated<T> {
    query: T,
    page: i64,
    per_page: i64,
}

impl<T> Paginated<T>
where
    T: QueryFragment<Sqlite>,
{
    pub fn per_page(self, per_page: i64) -> Self {
        Paginated { per_page, ..self }
    }

    pub fn load_and_count<U>(self, conn: &SqliteConnection) -> QueryResult<(Vec<U>, i64)>
    where
        Self: LoadQuery<SqliteConnection, (U, i64)>,
    {
        let results = self.load(conn)?;
        let total = results.get(0).map(|x| x.1).unwrap_or(0);
        let records = results.into_iter().map(|x| x.0).collect();
        Ok((records, total))
    }
}

impl<T: Query> Query for Paginated<T> {
    type SqlType = (T::SqlType, BigInt);
}

impl<T> RunQueryDsl<SqliteConnection> for Paginated<T> {}

impl<T> QueryFragment<Sqlite> for Paginated<T>
where
    T: QueryFragment<Sqlite>,
{
    fn walk_ast(&self, mut pass: AstPass<Sqlite>) -> QueryResult<()> {
        pass.push_sql("SELECT *, COUNT(*) OVER () FROM (");
        self.query.walk_ast(pass.reborrow())?;
        pass.push_sql(") t LIMIT ");
        pass.push_bind_param::<BigInt, _>(&self.per_page)?;
        pass.push_sql(" OFFSET ");
        let offset = (self.page - 1) * self.per_page;
        pass.push_bind_param::<BigInt, _>(&offset)?;

        Ok(())
    }
}
