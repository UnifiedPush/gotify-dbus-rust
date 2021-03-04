#![feature(proc_macro_hygiene, decl_macro)]

use std::error::Error;
use std::sync::{
    Arc,
    RwLock
};
use std::collections::HashMap;
use lazy_static::lazy_static;

mod registration;
mod server;

lazy_static! {
    static ref DBUS_CONNECTION: zbus::Connection = zbus::Connection::new_session().unwrap();
}

fn main() -> Result<(), Box<dyn Error>> {
    let push_connections = Arc::new(RwLock::new(HashMap::new()));

    let push_connections_ = push_connections.clone();
    std::thread::spawn(move || {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(server::run(push_connections_, &*DBUS_CONNECTION.inner()));
    });

    registration::run(push_connections, &*DBUS_CONNECTION)?;

    Ok(())
}