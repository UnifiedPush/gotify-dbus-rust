use std::sync::{Arc, RwLock};
use std::collections::HashMap;

async fn send_push(conn: &zbus::azync::Connection, appid: &str, token: &str, data: &str) {
    if let Ok(_) = conn.send_message(zbus::Message::method(
        Some("org.unifiedpush.Connector.vurpo_poc"), 
        Some(appid),
        "/org/unifiedpush/Connector/vurpo_poc",
        Some("org.unifiedpush.Connector1"),
        "Message",
        &(token, data, "")).unwrap()).await {
        println!("PUSH MESSAGE: {}: {}", appid, data);
    } else {
        println!("PUSH MESSAGE ERROR: {}: {}", appid, data)
    }
}

#[derive(Clone)]
struct PushState {
    push_connections: Arc<RwLock<HashMap<String, String>>>,
    dbus_connection: &'static zbus::azync::Connection
}

// Separated this part into its own function, to avoid the error when having an RwLockReadGuard in an async function
fn get_connection(map: &Arc<RwLock<HashMap<String, String>>>, token: &str) -> Result<Option<String>, ()> {
    if let Ok(connections) = map.read() {
        return Ok(connections.get(token).map(|c| c.to_owned()))
    } else {
        return Err(())
    }
}

// token: &RawStr, state: State<PushState, 'static>, data: String
async fn handle_push(mut req: tide::Request<PushState>) -> tide::Result<tide::StatusCode> {
    let data = req.body_string().await;
    let state = req.state();
    let token = req.param("token").unwrap(); // unwrap because this is always valid

    let connection = get_connection(&state.push_connections, token);

    match connection {
        Err(_) => Err(tide::Error::from_str(500, "Internal server error")),
        Ok(None) => Err(tide::Error::from_str(404, "Not found")),
        Ok(Some(appid)) => {
            if let Ok(data) = data {
                send_push(state.dbus_connection, &appid, token, &data).await;
                Ok(tide::StatusCode::Ok)
            } else {
                Err(tide::Error::from_str(400, "Malformed UTF-8"))
            }
        }
    }
}

pub async fn run<'a>(push_connections: Arc<RwLock<HashMap<String, String>>>, dbus_connection: &'static zbus::azync::Connection) {
    let state = PushState{
        push_connections: push_connections,
        dbus_connection: dbus_connection
    };

    let mut app = tide::with_state(state);
    app.at("/:token").post(handle_push);
    app.listen("0.0.0.0:8777").await;
}