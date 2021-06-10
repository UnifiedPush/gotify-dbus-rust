#![feature(proc_macro_hygiene, decl_macro)]

use std::error::Error as StdError;
use std::sync::Arc;
use std::fs::File;
use std::path::PathBuf;
use lazy_static::lazy_static;
use directories_next::ProjectDirs;
use r2d2_sqlite::SqliteConnectionManager;
use r2d2_sqlite::rusqlite::params;
use log::{error, warn, info, debug, trace};

mod registration;
mod gotify_receiver;

use unifiedpush_gotify_lib::LoginFile;

#[derive(Debug)]
enum Error {
    CantReadConfigFile,
    StdError(Box<dyn StdError>)
}
impl<T: 'static> From<T> for Error where T: StdError {
    fn from(error: T) -> Self {
        Error::StdError(Box::new(error))
    }
}

lazy_static! {
    static ref DBUS_CONNECTION: zbus::Connection = zbus::Connection::new_session().unwrap();
}

fn main() -> Result<(), Error> {
    env_logger::init();
    info!("Starting UnifiedPush Gotify receiver");

    let project_dirs = ProjectDirs::from("fi", "vurpo", "UnifiedPushGotify")
        .ok_or(Error::CantReadConfigFile)?;
    let config_dir = project_dirs.config_dir();
    let login_file_path = {
        let mut buf = PathBuf::from(config_dir);
        buf.push("login.json");
        buf
    };
    let login_file: LoginFile = serde_json::from_reader(&File::open(&login_file_path)?)?;
    debug!("Login file loaded OK");

    let db_path = {
        let mut buf = PathBuf::from(config_dir);
        buf.push("database.db");
        buf
    };
    let sqlite_connection_manager = SqliteConnectionManager::file(&db_path);
    let sqlite_pool = Arc::new(r2d2::Pool::new(sqlite_connection_manager)?);
    debug!("Connection database loaded OK");

    sqlite_pool.get()?
        .execute("CREATE TABLE IF NOT EXISTS connections (appid TEXT NOT NULL, token TEXT NOT NULL, gotify_token TEXT NOT NULL, gotify_id INTEGER NOT NULL)", params![])?;

    sqlite_pool.get()?
        .execute("CREATE TABLE IF NOT EXISTS last_seen_message (message_id INTEGER NOT NULL)", params![])?;

    let sqlite_pool_ = sqlite_pool.clone();
    let login_file_ = login_file.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        debug!("Starting Gotify receiver thread");
        rt.block_on(gotify_receiver::run(sqlite_pool_, &*DBUS_CONNECTION.inner(), login_file_));
    });

    debug!("Starting D-Bus registration receiver thread");
    registration::run(sqlite_pool, &*DBUS_CONNECTION, login_file).map_err(|e| Error::StdError(e))
}