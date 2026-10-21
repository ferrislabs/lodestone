use common::CoreError;
use sqlx::{ConnectOptions, PgConnection, PgPool};

use crate::infrastructure::postgres::error::map_sqlx_error;

pub struct PgListenerLock {
    connection: PgConnection,
    key: i64,
    held: bool,
}

impl PgListenerLock {
    pub async fn connect(pool: &PgPool, key: i64) -> Result<Self, CoreError> {
        let connection = pool
            .connect_options()
            .connect()
            .await
            .map_err(map_sqlx_error)?;
        Ok(Self {
            connection,
            key,
            held: false,
        })
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    pub async fn try_acquire(&mut self) -> Result<bool, CoreError> {
        if self.held {
            return self.ping().await.map(|()| true);
        }
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(self.key)
            .fetch_one(&mut self.connection)
            .await
            .map_err(map_sqlx_error)?;
        self.held = acquired;
        Ok(acquired)
    }

    pub async fn ping(&mut self) -> Result<(), CoreError> {
        let result = sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&mut self.connection)
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                self.held = false;
                Err(map_sqlx_error(error))
            }
        }
    }

    pub async fn release(mut self) {
        if self.held {
            let _ = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
                .bind(self.key)
                .fetch_one(&mut self.connection)
                .await;
        }
        let _ = sqlx::Connection::close(self.connection).await;
    }
}
