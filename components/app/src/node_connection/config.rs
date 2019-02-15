use futures::channel::mpsc;
use proto::app_server::messages::{AppToAppServer};

pub struct AppConfig {
    sender: mpsc::Sender<AppToAppServer>,
}
