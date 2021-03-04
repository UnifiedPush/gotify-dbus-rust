use std::error::Error;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use zbus::Connection;
use zbus::{dbus_interface, fdo};

struct Distributor {
    connections: Arc<RwLock<HashMap<String, String>>> // token -> appid
}

#[dbus_interface(name = "org.unifiedpush.Distributor1")]
impl Distributor {
    fn register(&mut self, appid: &str, token: &str) -> (String, String) {
        if let Ok(mut conn) = self.connections.write() {
            conn.insert(token.to_string(), appid.to_string());
            eprintln!("REGISTER {}", appid);
            (
                "NEW_ENDPOINT".to_string(),
                format!("https://push.vurp.io/{}", token)
            )
        } else {
            (
                "REGISTRATION_FAILED".to_string(),
                "Internal provider error".to_string()
            )
        }
    }

    fn unregister(&mut self, appid: &str) {
        if let Ok(mut conn) = self.connections.write() {
            conn.retain(|_token, id| id != appid);
            eprintln!("UNREGISTER {}", appid);
        }
    }
}

pub fn run(push_connections: Arc<RwLock<HashMap<String, String>>>, dbus_conn: &'static Connection) -> Result<(), Box<dyn Error>> {
    fdo::DBusProxy::new(&dbus_conn)?.request_name(
        "org.unifiedpush.Distributor.vurpo_poc",
        fdo::RequestNameFlags::ReplaceExisting.into(),
    )?;

    let mut object_server = zbus::ObjectServer::new(&dbus_conn);


    let r = Distributor { connections: push_connections.clone() };
    object_server.at("/org/unifiedpush/Distributor", r)?;
    loop {
        if let Err(err) = object_server.try_handle_next() {
            eprintln!("{}", err);
        }
    }
}
