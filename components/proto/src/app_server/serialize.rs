use crate::serialize::SerializeError;
use super::messages::{AppServerToApp, AppToAppServer};

use crate::index_server::messages::IndexServerAddress;
use crate::funder::messages::RelayAddress;


pub fn serialize_app_server_to_app(app_server_to_app: &AppServerToApp<RelayAddress, IndexServerAddress>) -> Vec<u8> {
    // TODO
    unimplemented!();
}

pub fn deserialize_app_server_to_app(data: &[u8]) -> Result<AppServerToApp<RelayAddress, IndexServerAddress>, SerializeError> {
    // TODO
    unimplemented!();
}

pub fn serialize_app_to_app_server(app_server_to_app: &AppToAppServer<RelayAddress, IndexServerAddress>) -> Vec<u8> {
    // TODO
    unimplemented!();
}

pub fn deserialize_app_to_app_server(data: &[u8]) -> Result<AppToAppServer<RelayAddress, IndexServerAddress>, SerializeError> {
    // TODO
    unimplemented!();
}
