use serde::{ Serialize, Deserialize };
use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use r2d2_sqlite::rusqlite::params;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoginFile {
    pub gotify_base_url: String,
    pub gotify_device_token: String
}

#[derive(Deserialize, Debug)]
pub struct GotifyApplication {
    pub id: i32,
    pub name: String,
    pub token: String
}

#[derive(Debug)]
pub struct DbUpConnection {
    pub appid: String,
    pub token: String,
    pub gotify_id: i32,
    pub gotify_token: String,
}

pub fn get_connections_with_token<'a>(pool: &'a r2d2::Pool<SqliteConnectionManager>, token: &str) -> Result<Vec<DbUpConnection>, Box<dyn std::error::Error + Send + Sync>> {
    let conn = pool.get()?;
    let mut s = conn.prepare("SELECT * FROM connections WHERE token=?")?;
    let mut rows = s.query(&[token])?;

    let mut result = Vec::new();
    while let Ok(Some(row)) = rows.next() {
        result.push(DbUpConnection {
            appid: row.get_unwrap("appid"),
            token: row.get_unwrap("token"),
            gotify_id: row.get_unwrap("gotify_id"),
            gotify_token: row.get_unwrap("gotify_token")
        });
    }

    Ok(result)
}

pub fn get_all_connections<'a>(pool: &'a r2d2::Pool<SqliteConnectionManager>) -> Result<Vec<DbUpConnection>, Box<dyn std::error::Error + Send + Sync>> {
    let conn = pool.get()?;
    let mut s = conn.prepare("SELECT * FROM connections")?;
    let mut rows = s.query(params![])?;

    let mut result = Vec::new();
    while let Ok(Some(row)) = rows.next() {
        result.push(DbUpConnection {
            appid: row.get_unwrap("appid"),
            token: row.get_unwrap("token"),
            gotify_id: row.get_unwrap("gotify_id"),
            gotify_token: row.get_unwrap("gotify_token")
        });
    }

    Ok(result)
}