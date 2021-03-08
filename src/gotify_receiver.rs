use std::sync::Arc;

use unifiedpush_gotify_lib::{
    LoginFile,
    GotifyApplication,
    get_all_connections
};
use r2d2_sqlite::SqliteConnectionManager;
use r2d2_sqlite::rusqlite::params;
use futures_util::stream::StreamExt;
use url::Url;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct GotifyMessage {
    id: i32,
    appid: i32,
    message: String,
}

#[derive(Deserialize, Debug)]
struct GotifyMessagesList {
    messages: Vec<GotifyMessage>
}

async fn send_push(conn: &zbus::azync::Connection, appid: &str, token: &str, data: &str) {
    if let Ok(_) = conn.send_message(zbus::Message::method(
        Some("org.unifiedpush.Connector.gotify"), 
        Some(appid),
        "/org/unifiedpush/Connector",
        Some("org.unifiedpush.Connector1"),
        "Message",
        &(token, data, "")).unwrap()).await {
        eprintln!("Push message: {}: {}", appid, data);
    } else {
        eprintln!("Sending Message failed: {}: {}", appid, data)
    }
}

async fn send_unregister(conn: &zbus::azync::Connection, appid: &str, token: &str) {
    if let Ok(_) = conn.send_message(zbus::Message::method(
        Some("org.unifiedpush.Distributor.gotify"),
        Some(appid),
        "/org/unifiedpush/Connector",
        Some("org.unifiedpush.Connector1"),
        "Unregister",
        &(token)).unwrap()).await {
        eprintln!("Unregistered from Gotify: {}", appid);
    } else {
        eprintln!("Sending Unregister failed: {}", appid);
    }
}

async fn check_removed_apps(
    sqlite_pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
    dbus_conn: &'static zbus::azync::Connection,
    login_file: LoginFile) {
    let client = reqwest::Client::new();
    loop {
        let response = client.get(format!("{}/application", &login_file.gotify_base_url).as_str())
            .header("X-Gotify-Key", &login_file.gotify_device_token)
            .send().await;
        if let Ok(r) = response.and_then(|r| Ok(r.json::<Vec<GotifyApplication>>())) {
            if let Ok(gotify_connections) = r.await {
                if let Ok(db_connections) = get_all_connections(&sqlite_pool) {
                    for connection in db_connections {
                        if let None = gotify_connections.iter().find(|a| a.id == connection.gotify_id) {
                            send_unregister(dbus_conn, &connection.appid, &connection.token).await;
                            sqlite_pool.get().map_err(|_|())
                                .and_then(|c| c.execute("DELETE FROM connections WHERE token=?", &[connection.token]).map_err(|_|()));
                        }
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
    }
}

fn update_last_seen(pool: &r2d2::Pool<SqliteConnectionManager>, message_id: i32) {
    if let Ok(c) = pool.get() {
        if let Err(QueryReturnedNoRows) = c.query_row("SELECT message_id FROM last_seen_message", params![], |r| r.get::<_, i32>(0)) {
            c.execute("INSERT INTO last_seen_message VALUES (?)", params![message_id]);
        } else {
            c.execute("UPDATE last_seen_message SET message_id = MAX(message_id, ?)", &[message_id]);
        }
    }
}

/// Returns whether or not the message was a UP push message that we handled.
async fn handle_message(
    pool: &r2d2::Pool<SqliteConnectionManager>,
    dbus_connection: &'static zbus::azync::Connection,
    message: GotifyMessage) -> bool {
    if let Ok(conn) = pool.get().map_err(|_|()) {
        if let Ok(row) = conn.query_row(
                "SELECT * FROM connections WHERE gotify_id = ?",
                &[message.appid],
                |r| Ok((r.get_unwrap::<_, String>("appid"), r.get_unwrap::<_, String>("token")))
        ).map_err(|_|()) {
            send_push(dbus_connection, &row.0, &row.1, &message.message).await;
            update_last_seen(pool, message.id);
            true
        } else {
            update_last_seen(pool, message.id);
            false
        }
    } else {
        false
    }
}

async fn delete_message(
    message_id: i32,
    login_file: &LoginFile
) {
    let client = reqwest::Client::new();
    client.delete(format!("{}/message/{}", login_file.gotify_base_url, message_id).as_str())
        .header("X-Gotify-Key", &login_file.gotify_device_token)
        .send().await;
}

async fn check_for_missed_messages(
    pool: &r2d2::Pool<SqliteConnectionManager>,
    dbus_connection: &'static zbus::azync::Connection,
    login_file: LoginFile) {
    eprintln!("Checking for missed messages");
    if let Some(last_seen_message) = pool.get().ok().and_then(|c| c.query_row("SELECT message_id FROM last_seen_message", params![], |r| r.get::<_, i32>(0)).ok()) {
        let client = reqwest::Client::new();
        let response = client.get(format!("{}/message", &login_file.gotify_base_url).as_str())
            .header("X-Gotify-Key", &login_file.gotify_device_token)
            .send().await;
        if let Ok(r) = response.and_then(|r| Ok(r.json::<GotifyMessagesList>())) {
            if let Ok(mut messages) = r.await {
                messages.messages.sort_by_key(|m| m.id);
                for message in messages.messages {
                    if message.id > last_seen_message {
                        let id = message.id;
                        if handle_message(pool, dbus_connection, message).await {
                            delete_message(id, &login_file).await;
                        }
                    }
                }
            }
        }
    }
}

pub async fn run(
    sqlite_pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
    dbus_connection: &'static zbus::azync::Connection,
    login_file: LoginFile) {
    
    let base_url = Url::parse(&login_file.gotify_base_url).expect("Gotify base URL not a valid URL");
    let websocket_url = {
        let mut ws_url = base_url.clone();
        if base_url.scheme() == "https" { ws_url.set_scheme("wss").unwrap(); }
        else if base_url.scheme() == "http" { ws_url.set_scheme("ws").unwrap(); }
        else { panic!("Only HTTPS and HTTP URLs supported"); }
        ws_url.path_segments_mut().unwrap().push("stream");
        ws_url.query_pairs_mut().append_pair("token", &login_file.gotify_device_token);
        ws_url
    };

    eprintln!("{}", websocket_url);

    let sqlite_pool_ = sqlite_pool.clone();
    tokio::spawn(check_removed_apps(sqlite_pool_, dbus_connection, login_file.clone()));

    loop {
        check_for_missed_messages(&sqlite_pool, dbus_connection, login_file.clone()).await;

        if let Ok((mut ws_stream, _)) = tokio_tungstenite::connect_async(&websocket_url).await {
            eprintln!("Connected to Gotify");
            
            while let Ok(message) = tokio::time::timeout(std::time::Duration::from_secs(50), ws_stream.next()).await {
                if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) = message {
                    if let Ok(message) = serde_json::from_str::<GotifyMessage>(&text) {
                        let id = message.id;
                        if handle_message(&sqlite_pool, dbus_connection, message).await {
                            delete_message(id, &login_file).await;
                        }
                    }
                }
            }
            eprintln!("Timed out!");
            ws_stream.close(None).await;
        } else {
            eprintln!("Failed to connect to Gotify! Retrying in 10 seconds");
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    }
}