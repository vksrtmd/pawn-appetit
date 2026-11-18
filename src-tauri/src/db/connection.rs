use std::time::Duration;

use dashmap::mapref::entry::Entry;
use diesel::{
    connection::SimpleConnection,
    r2d2::{ConnectionManager, Pool},
    RunQueryDsl, SqliteConnection,
};

use crate::{error::Result, AppState};

const PRAGMA_JOURNAL_MODE_DELETE: &str =
    include_str!("../../../database/pragmas/journal_mode_delete.sql");
const PRAGMA_JOURNAL_MODE_OFF: &str =
    include_str!("../../../database/pragmas/journal_mode_off.sql");
const PRAGMA_FOREIGN_KEYS_ON: &str = include_str!("../../../database/pragmas/foreign_keys_on.sql");
const PRAGMA_FOREIGN_KEYS_OFF: &str =
    include_str!("../../../database/pragmas/foreign_keys_off.sql");
const PRAGMA_BUSY_TIMEOUT: &str = include_str!("../../../database/pragmas/busy_timeout.sql");

#[derive(Debug)]
pub enum JournalMode {
    Delete,
    Off,
}

#[derive(Debug)]
pub struct ConnectionOptions {
    pub journal_mode: JournalMode,
    pub enable_foreign_keys: bool,
    pub busy_timeout: Option<Duration>,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            journal_mode: JournalMode::Delete,
            enable_foreign_keys: true,
            busy_timeout: Some(Duration::from_secs(30)),
        }
    }
}

impl ConnectionOptions {
    /// The importer's own connection: no journal and no foreign keys, so a
    /// bulk load is fast. It never joins the shared pool, which keeps the
    /// safe defaults.
    pub fn import() -> Self {
        Self {
            journal_mode: JournalMode::Off,
            enable_foreign_keys: false,
            busy_timeout: None,
        }
    }

    pub fn connect(&self, db_path: &str) -> Result<SqliteConnection> {
        use diesel::Connection;
        let mut conn = SqliteConnection::establish(db_path)?;
        self.configure(&mut conn)?;
        Ok(conn)
    }

    fn configure(&self, conn: &mut SqliteConnection) -> diesel::result::QueryResult<()> {
        match self.journal_mode {
            JournalMode::Delete => conn.batch_execute(PRAGMA_JOURNAL_MODE_DELETE)?,
            JournalMode::Off => conn.batch_execute(PRAGMA_JOURNAL_MODE_OFF)?,
        }
        // Explicit either way: the SQLite build's own default may be on.
        conn.batch_execute(if self.enable_foreign_keys {
            PRAGMA_FOREIGN_KEYS_ON
        } else {
            PRAGMA_FOREIGN_KEYS_OFF
        })?;
        if let Some(d) = self.busy_timeout {
            conn.batch_execute(&PRAGMA_BUSY_TIMEOUT.replace("{0}", &d.as_millis().to_string()))?;
        }
        Ok(())
    }
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error>
    for ConnectionOptions
{
    fn on_acquire(
        &self,
        conn: &mut SqliteConnection,
    ) -> std::result::Result<(), diesel::r2d2::Error> {
        self.configure(conn)
            .map_err(diesel::r2d2::Error::QueryError)
    }
}

pub type PooledSqliteConnection =
    diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<diesel::SqliteConnection>>;

/// Add the game end-time columns to databases created before they existed.
/// Older files must keep working without a full migration step.
// diesel's QueryableByName derive expands to `field: field`.
#[allow(clippy::redundant_field_names)]
fn ensure_game_time_columns(conn: &mut SqliteConnection) -> diesel::result::QueryResult<()> {
    use diesel::sql_types::BigInt;
    use diesel::{QueryableByName, RunQueryDsl};

    #[derive(QueryableByName)]
    struct ColumnCounts {
        #[diesel(sql_type = BigInt)]
        tables: i64,
        #[diesel(sql_type = BigInt)]
        end_date: i64,
        #[diesel(sql_type = BigInt)]
        end_time: i64,
    }

    let counts: ColumnCounts = diesel::sql_query(
        "SELECT \
         (SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'Games') AS tables, \
         (SELECT count(*) FROM pragma_table_info('Games') WHERE name = 'EndDate') AS end_date, \
         (SELECT count(*) FROM pragma_table_info('Games') WHERE name = 'EndTime') AS end_time",
    )
    .get_result(conn)?;
    if counts.tables == 0 {
        return Ok(());
    }
    if counts.end_date == 0 {
        conn.batch_execute("ALTER TABLE Games ADD COLUMN EndDate TEXT")?;
    }
    if counts.end_time == 0 {
        conn.batch_execute("ALTER TABLE Games ADD COLUMN EndTime TEXT")?;
    }
    Ok(())
}

/// What the game list sorts and filters by: when the game ended, else began.
pub(crate) const GAME_DATE_EXPR: &str = "COALESCE(Games.EndDate, Games.Date)";
pub(crate) const GAME_TIME_EXPR: &str = "COALESCE(Games.EndTime, Games.UTCTime)";

/// Index the game list's date sort and date filters read.
fn games_date_index_sql() -> String {
    // An index expression cannot qualify its columns; the planner still
    // matches it to the qualified form the list query uses.
    format!(
        "CREATE INDEX IF NOT EXISTS games_date_expr_idx ON Games({}, {})",
        GAME_DATE_EXPR.replace("Games.", ""),
        GAME_TIME_EXPR.replace("Games.", "")
    )
}

fn ensure_game_date_index(conn: &mut SqliteConnection) -> diesel::result::QueryResult<()> {
    let has_games: i64 = diesel::select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
        "(SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'Games')",
    ))
    .get_result(conn)?;
    if has_games > 0 {
        conn.batch_execute(&games_date_index_sql())?;
    }
    Ok(())
}

/// The pool for one database file: connections open on demand, always with
/// the safe default settings, and the file's schema upgrades have run.
pub(crate) fn open_pool(db_path: &str) -> Result<Pool<ConnectionManager<SqliteConnection>>> {
    let pool = Pool::builder()
        .max_size(16)
        .min_idle(Some(0))
        .connection_customizer(Box::new(ConnectionOptions::default()))
        .build(ConnectionManager::<SqliteConnection>::new(db_path))?;
    let mut conn = pool.get()?;
    ensure_game_time_columns(&mut conn)?;
    ensure_game_date_index(&mut conn)?;
    Ok(pool)
}

pub(crate) fn get_db_or_create(state: &AppState, db_path: &str) -> Result<PooledSqliteConnection> {
    let pool = match state.connection_pool.entry(db_path.to_string()) {
        Entry::Occupied(entry) => entry.get().clone(),
        Entry::Vacant(entry) => entry.insert(open_pool(db_path)?).clone(),
    };
    Ok(pool.get()?)
}
