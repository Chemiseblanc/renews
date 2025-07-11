use super::{
    Message, Storage, StringStream, StringTimestampStream, U64Stream,
    common::{Headers, extract_message_id},
};
use async_stream::stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use smallvec::SmallVec;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::error::Error;
use std::str::FromStr;

// SQL schemas for PostgreSQL storage
const MESSAGES_TABLE: &str = "CREATE TABLE IF NOT EXISTS messages (
        message_id TEXT PRIMARY KEY,
        headers TEXT,
        body TEXT,
        size BIGINT NOT NULL
    )";

const GROUP_ARTICLES_TABLE: &str = "CREATE TABLE IF NOT EXISTS group_articles (
        group_name TEXT,
        number BIGINT,
        message_id TEXT,
        inserted_at BIGINT NOT NULL,
        PRIMARY KEY(group_name, number),
        FOREIGN KEY(message_id) REFERENCES messages(message_id)
    )";

const GROUPS_TABLE: &str = "CREATE TABLE IF NOT EXISTS groups (
        name TEXT PRIMARY KEY,
        created_at BIGINT NOT NULL,
        moderated BOOLEAN NOT NULL DEFAULT FALSE
    )";

#[derive(Clone)]
pub struct PostgresStorage {
    pool: PgPool,
}

impl PostgresStorage {
    #[tracing::instrument(skip_all)]
    /// Create a new Postgres storage backend.
    pub async fn new(uri: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let opts = PgConnectOptions::from_str(uri).map_err(|e| {
            format!(
                "Invalid PostgreSQL connection URI '{}': {}

Please ensure the URI is in the correct format:
- Standard connection: postgresql://user:password@host:port/database
- Local connection: postgresql:///database_name
- With SSL: postgresql://user:password@host:port/database?sslmode=require

Required connection components:
- host: PostgreSQL server hostname or IP
- port: PostgreSQL server port (default: 5432)
- database: Target database name
- user: PostgreSQL username
- password: User password (if required)",
                uri, e
            )
        })?;
        
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await
            .map_err(|e| {
                format!(
                    "Failed to connect to PostgreSQL database '{}': {}

Possible causes:
- PostgreSQL server is not running or unreachable
- Incorrect hostname, port, username, or password
- Database does not exist
- Connection refused due to pg_hba.conf configuration
- SSL/TLS connection required but not configured
- Network firewall blocking the connection
- PostgreSQL server not accepting connections

Please verify:
1. PostgreSQL server is running: systemctl status postgresql
2. Database exists: psql -l
3. User has access privileges: GRANT CONNECT ON DATABASE dbname TO username;
4. Connection settings in pg_hba.conf allow your connection method",
                    uri, e
                )
            })?;

        // Create database schema
        sqlx::query(MESSAGES_TABLE).execute(&pool).await.map_err(|e| {
            format!("Failed to create messages table in PostgreSQL database '{}': {}", uri, e)
        })?;
        sqlx::query(GROUP_ARTICLES_TABLE).execute(&pool).await.map_err(|e| {
            format!("Failed to create group_articles table in PostgreSQL database '{}': {}", uri, e)
        })?;
        sqlx::query(GROUPS_TABLE).execute(&pool).await.map_err(|e| {
            format!("Failed to create groups table in PostgreSQL database '{}': {}", uri, e)
        })?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl Storage for PostgresStorage {
    #[tracing::instrument(skip_all)]
    async fn store_article(&self, article: &Message) -> Result<(), Box<dyn Error + Send + Sync>> {
        let msg_id = extract_message_id(article).ok_or("missing Message-ID")?;
        let headers = serde_json::to_string(&Headers(article.headers.clone()))?;

        // Store the message once
        sqlx::query(
            "INSERT INTO messages (message_id, headers, body, size) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(&msg_id)
        .bind(&headers)
        .bind(&article.body)
        .bind(i64::try_from(article.body.len()).unwrap_or(i64::MAX))
        .execute(&self.pool)
        .await?;

        // Extract newsgroups from headers
        let newsgroups: SmallVec<[String; 4]> = article
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
            .map(|(_, v)| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect::<SmallVec<[String; 4]>>()
            })
            .unwrap_or_default();

        // Associate with each group
        let now = chrono::Utc::now().timestamp();
        for group in newsgroups {
            let next: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(number),0)+1 FROM group_articles WHERE group_name = $1",
            )
            .bind(&group)
            .fetch_one(&self.pool)
            .await?;

            sqlx::query(
                "INSERT INTO group_articles (group_name, number, message_id, inserted_at) VALUES ($1, $2, $3, $4)",
            )
            .bind(&group)
            .bind(next)
            .bind(&msg_id)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn get_article_by_number(
        &self,
        group: &str,
        number: u64,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        if let Some(row) = sqlx::query(
            "SELECT m.headers, m.body FROM messages m JOIN group_articles g ON m.message_id = g.message_id WHERE g.group_name = $1 AND g.number = $2",
        )
        .bind(group)
        .bind(i64::try_from(number).unwrap_or(-1))
        .fetch_optional(&self.pool)
        .await?
        {
            let headers_str: String = row.try_get("headers")?;
            let body: String = row.try_get("body")?;
            let Headers(headers) = serde_json::from_str(&headers_str)?;
            Ok(Some(Message { headers, body }))
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_article_by_id(
        &self,
        message_id: &str,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        if let Some(row) = sqlx::query("SELECT headers, body FROM messages WHERE message_id = $1")
            .bind(message_id)
            .fetch_optional(&self.pool)
            .await?
        {
            let headers_str: String = row.try_get("headers")?;
            let body: String = row.try_get("body")?;
            let Headers(headers) = serde_json::from_str(&headers_str)?;
            Ok(Some(Message { headers, body }))
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(skip_all)]
    async fn add_group(
        &self,
        group: &str,
        moderated: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO groups (name, created_at, moderated) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(group)
        .bind(now)
        .bind(moderated)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn remove_group(&self, group: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM group_articles WHERE group_name = $1")
            .bind(group)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM groups WHERE name = $1")
            .bind(group)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "DELETE FROM messages WHERE message_id NOT IN (SELECT DISTINCT message_id FROM group_articles)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn is_group_moderated(&self, group: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let row = sqlx::query("SELECT moderated FROM groups WHERE name = $1")
            .bind(group)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(r) = row {
            let m: bool = r.try_get("moderated")?;
            Ok(m)
        } else {
            Ok(false)
        }
    }

    #[tracing::instrument(skip_all)]
    async fn group_exists(&self, group: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let row = sqlx::query("SELECT 1 FROM groups WHERE name = $1 LIMIT 1")
            .bind(group)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    #[tracing::instrument(skip_all)]
    fn list_groups(&self) -> StringStream<'_> {
        let pool = self.pool.clone();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT name FROM groups ORDER BY name")
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("name") {
                        Ok(name) => yield Ok(name),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_groups_since(&self, since: chrono::DateTime<chrono::Utc>) -> StringStream<'_> {
        let pool = self.pool.clone();
        let timestamp = since.timestamp();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT name FROM groups WHERE created_at > $1 ORDER BY name")
                .bind(timestamp)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("name") {
                        Ok(name) => yield Ok(name),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_groups_with_times(&self) -> StringTimestampStream<'_> {
        let pool = self.pool.clone();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT name, created_at FROM groups ORDER BY name")
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => {
                        match (r.try_get::<String, _>("name"), r.try_get::<i64, _>("created_at")) {
                            (Ok(name), Ok(ts)) => yield Ok((name, ts)),
                            (Err(e), _) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                            (_, Err(e)) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                        }
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_article_numbers(&self, group: &str) -> U64Stream<'_> {
        let pool = self.pool.clone();
        let group = group.to_string();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT number FROM group_articles WHERE group_name = $1 ORDER BY number")
                .bind(&group)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<i64, _>("number") {
                        Ok(number) => yield Ok(u64::try_from(number).unwrap_or(0)),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_article_ids(&self, group: &str) -> StringStream<'_> {
        let pool = self.pool.clone();
        let group = group.to_string();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT message_id FROM group_articles WHERE group_name = $1 ORDER BY number")
                .bind(&group)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("message_id") {
                        Ok(message_id) => yield Ok(message_id),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_article_ids_since(
        &self,
        group: &str,
        since: chrono::DateTime<chrono::Utc>,
    ) -> StringStream<'_> {
        let pool = self.pool.clone();
        let group = group.to_string();
        let timestamp = since.timestamp();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT message_id FROM group_articles WHERE group_name = $1 AND inserted_at > $2 ORDER BY number")
                .bind(&group)
                .bind(timestamp)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("message_id") {
                        Ok(message_id) => yield Ok(message_id),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    async fn purge_group_before(
        &self,
        group: &str,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM group_articles WHERE group_name = $1 AND inserted_at < $2")
            .bind(group)
            .bind(before.timestamp())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn purge_orphan_messages(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query(
            "DELETE FROM messages WHERE message_id NOT IN (SELECT DISTINCT message_id FROM group_articles)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn get_message_size(
        &self,
        message_id: &str,
    ) -> Result<Option<u64>, Box<dyn Error + Send + Sync>> {
        if let Some(row) = sqlx::query("SELECT size FROM messages WHERE message_id = $1")
            .bind(message_id)
            .fetch_optional(&self.pool)
            .await?
        {
            let size: i64 = row.try_get("size")?;
            Ok(Some(u64::try_from(size).unwrap_or(0)))
        } else {
            Ok(None)
        }
    }

    async fn delete_article_by_id(
        &self,
        message_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM group_articles WHERE message_id = $1")
            .bind(message_id)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "DELETE FROM messages WHERE message_id = $1 AND NOT EXISTS (SELECT 1 FROM group_articles WHERE message_id = $1)",
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
