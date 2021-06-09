use std::error::Error;
use std::sync::Arc;

use r2d2_sqlite::SqliteConnectionManager;
use unifiedpush_gotify_lib::{
    LoginFile,
    GotifyApplication,
    get_connections_with_token,
};
use zbus::Connection;
use zbus::{dbus_interface, fdo};
use serde_json::json;
use r2d2_sqlite::rusqlite::params;

struct Distributor {
    dbus_conn: &'static Connection,
    gotify_login: LoginFile,
    sqlite_pool: Arc<r2d2::Pool<SqliteConnectionManager>> // token -> appid
}

fn send_new_endpoint(conn: &zbus::Connection, appid: &str, token: &str, endpoint: &str) {
    conn.send_message(zbus::Message::method(
        Some("org.unifiedpush.Distributor.gotify"), 
        Some(appid),
        "/org/unifiedpush/Connector",
        Some("org.unifiedpush.Connector1"),
        "NewEndpoint",
        &(token, endpoint)).unwrap());
}


#[dbus_interface(name = "org.unifiedpush.Distributor1")]
impl Distributor {
    fn register(&mut self, appid: &str, token: &str) -> (String, String) {
        let client = reqwest::blocking::Client::new();
        // Check if app already exists on Gotify
        if let Ok(list) = get_connections_with_token(&self.sqlite_pool, token).map_err(|_|()) {
            if let Some(c) = list.iter().find(|c| c.appid == appid && c.token == token) {
                send_new_endpoint(self.dbus_conn, appid, token, format!("{}/UP?token={}", self.gotify_login.gotify_base_url, c.gotify_token).as_str());
                (
                    "NEW_ENDPOINT".to_string(),
                    String::new()
                )
            } else {
                // Add new app to Gotify server
                let response = client.post(format!("{}/application", self.gotify_login.gotify_base_url).as_str())
                    .header("Content-Type", "application/json")
                    .header("X-Gotify-Key", &self.gotify_login.gotify_device_token)
                    .json(&json!{{
                        "name": appid,
                        "token": token
                    }})
                    .send()
                    .map_err(|_| ())
                    .and_then(|r| if r.status() == 200 { r.json::<GotifyApplication>().map_err(|_| ()) } else { Err(()) });
                if let Ok(response) = response {
                    // Try writing to sqlite database
                    if let Ok(_) = self.sqlite_pool.get()
                        .map(|c| c.execute("INSERT INTO connections (appid, token, gotify_token, gotify_id) VALUES (?, ?, ?, ?)", params![appid, token, response.token, response.id])) {
                        eprintln!("Register new app {}", appid);
                        send_new_endpoint(self.dbus_conn, appid, token, format!("{}/UP?token={}", self.gotify_login.gotify_base_url, response.token).as_str());
                        (
                            "NEW_ENDPOINT".to_string(),
                            String::new()
                        )
                    } else {
                        eprintln!("Writing to sqlite database failed!");
                        // Delete the newly added connection from Gotify, because writing to sqlite failed
                        client.delete(format!("{}/application/{}", self.gotify_login.gotify_base_url, response.id).as_str())
                            .header("X-Gotify-Key", &self.gotify_login.gotify_device_token)
                            .send();
                        (
                            "REGISTRATION_FAILED".to_owned(),
                            "Distributor database error".to_owned()
                        )
                    }
                } else {
                    (
                        "REGISTRATION_FAILED".to_owned(),
                        "Gotify server error".to_owned()
                    )
                }
            }
        } else {
            (
                "REGISTRATION_FAILED".to_owned(),
                "Distributor database error".to_owned()
            )
        }
    }

    fn unregister(&mut self, #[zbus(header)] _header: zbus::MessageHeader<'_>, token: &str) {
        let client = reqwest::blocking::Client::new();
        if let Ok(list) = get_connections_with_token(&self.sqlite_pool, token) {
            for row in list {
                client.delete(format!("{}/application/{}", self.gotify_login.gotify_base_url, row.gotify_id).as_str())
                    .header("X-Gotify-Key", &self.gotify_login.gotify_device_token)
                    .send();
            }
            self.sqlite_pool.get().map_err(|_|())
                .and_then(|c| c.execute("DELETE FROM connections WHERE token=?", &[token]).map_err(|_|()));
        }
    }
}

pub fn run(
    sqlite_pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
    dbus_conn: &'static Connection,
    login_file: LoginFile) -> Result<(), Box<dyn Error>> {
    fdo::DBusProxy::new(&dbus_conn)?.request_name(
        "org.unifiedpush.Distributor.gotify",
        fdo::RequestNameFlags::ReplaceExisting.into(),
    )?;

    let mut object_server = zbus::ObjectServer::new(&dbus_conn);



    let r = Distributor {
        dbus_conn: dbus_conn,
        gotify_login: login_file,
        sqlite_pool: sqlite_pool
    };
    object_server.at("/org/unifiedpush/Distributor", r)?;
    loop {
        if let Err(err) = object_server.try_handle_next() {
            eprintln!("{}", err);
        }
    }
}
