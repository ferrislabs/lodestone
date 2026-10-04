mod support;

use lodestone_core::{CtCursors, infrastructure::ct_cursor::postgres::PgCtCursors};
use sqlx::PgPool;
use support::at;

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_log_has_no_cursor(pool: PgPool) {
    let cursors = PgCtCursors::new(pool);
    assert_eq!(cursors.next_index("log-a").await.unwrap(), None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn cursor_only_moves_forward(pool: PgPool) {
    let cursors = PgCtCursors::new(pool);
    cursors.advance("log-a", 512, at(0)).await.unwrap();
    cursors.advance("log-a", 256, at(1)).await.unwrap();
    assert_eq!(cursors.next_index("log-a").await.unwrap(), Some(512));
    cursors.advance("log-a", 768, at(2)).await.unwrap();
    assert_eq!(cursors.next_index("log-a").await.unwrap(), Some(768));
}

#[sqlx::test(migrations = "../../migrations")]
async fn logs_have_independent_cursors(pool: PgPool) {
    let cursors = PgCtCursors::new(pool);
    cursors.advance("log-a", 256, at(0)).await.unwrap();
    cursors.advance("log-b", 1024, at(0)).await.unwrap();
    assert_eq!(cursors.next_index("log-a").await.unwrap(), Some(256));
    assert_eq!(cursors.next_index("log-b").await.unwrap(), Some(1024));
}
