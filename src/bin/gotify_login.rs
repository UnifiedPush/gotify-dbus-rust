use std::error::Error;
use std::path::PathBuf;
use std::fs::File;
use directories_next::ProjectDirs;
use clap::Clap;
use serde_json::json;
use serde::Deserialize;

use unifiedpush_gotify_lib::LoginFile;

#[derive(Debug)]
enum LoginError {
    NoConfigDirectory,
    InvalidServerOrCredentials,
    InvalidServerResponse,
    CantWriteLoginFile,
    StdError(Box<dyn Error>)
}
impl<T: 'static> From<T> for LoginError where T: Error {
    fn from(error: T) -> Self {
        LoginError::StdError(Box::new(error))
    }
}

#[derive(Clap)]
#[clap(version = "0.1", author = "vurpo")]
struct Options {
    #[clap(long = "url")]
    url: String,
    #[clap(long = "username")]
    username: String
}

#[derive(Debug, Deserialize)]
struct GotifyResponse {
    token: String
}

#[tokio::main]
async fn main() -> Result<(), LoginError> {
    let opts: Options = Options::parse();

    let project_dirs = ProjectDirs::from("fi", "vurpo", "UnifiedPushGotify")
        .ok_or(LoginError::NoConfigDirectory)?;
    let config_dir = project_dirs.config_dir();
    std::fs::create_dir_all(config_dir)?;
    
    let password = rpassword::read_password_from_tty(Some("Password: "))?;

    let client = reqwest::Client::new();
    if let Ok(new_device) = client.post(&format!("{}/client", opts.url.trim_end_matches("/")))
        .header("Content-Type", "application/json")
        .basic_auth(opts.username, Some(password))
        .json(&json!{{"name": "UnifiedPush-dbus-Gotify"}})
        .send().await
        .map_err(|_| ())
        .and_then(|r| if r.status() == 200 { Ok(r) } else { Err(()) }) {
            if let Ok(response) = new_device.json::<GotifyResponse>().await {
                let config = crate::LoginFile {
                    gotify_base_url: opts.url.trim_end_matches("/").to_owned(),
                    gotify_device_token: response.token
                };
                let login_file_path = {
                    let mut buf = PathBuf::from(config_dir);
                    buf.push("login.json");
                    buf
                };
                if let Ok(file) = File::create(login_file_path.as_path()) {
                    if let Ok(_) = serde_json::to_writer(&file, &config) {
                        println!("Successfully signed in to Gotify!");
                        Ok(())
                    } else {
                        Err(LoginError::CantWriteLoginFile)
                    }
                } else {
                    Err(LoginError::CantWriteLoginFile)
                }
            } else {
                Err(LoginError::InvalidServerResponse)
            }
    } else {
        Err(LoginError::InvalidServerOrCredentials)
    }
}