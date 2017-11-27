extern crate futures;

use self::futures::sync::mpsc;
use self::futures::Future;

use ::service_client::{ServiceClient, ServiceClientError};
use ::inner_messages::{ToSecurityModule, FromSecurityModule};
use ::crypto::identity::{PublicKey, Signature};

pub enum SecurityModuleClientError {
    RequestError(ServiceClientError),
    InvalidResponse,
}

pub struct SecurityModuleClient {
    service_client: ServiceClient<ToSecurityModule, FromSecurityModule>,
}

impl SecurityModuleClient {
    pub fn new(sender: mpsc::Sender<ToSecurityModule>, receiver: mpsc::Receiver<FromSecurityModule>) -> Self {
        SecurityModuleClient {
            service_client: ServiceClient::new(sender, receiver),
        }
    }

    pub fn request_public_key(&self) -> impl Future<Item=PublicKey,Error=SecurityModuleClientError> {
        self.service_client.request(ToSecurityModule::RequestPublicKey {})
            .map_err(|e| SecurityModuleClientError::RequestError(e))
            .and_then(|response| {
                match response {
                    FromSecurityModule::ResponsePublicKey {public_key} => 
                        Ok(public_key),
                    _ => Err(SecurityModuleClientError::InvalidResponse),
                }
            })
    }

    pub fn request_sign(&self, message: Vec<u8>) -> impl Future<Item=Signature, Error=SecurityModuleClientError> {
        self.service_client.request(ToSecurityModule::RequestSign {message})
            .map_err(|e| SecurityModuleClientError::RequestError(e))
            .and_then(|response| {
                match response {
                    FromSecurityModule::ResponseSign {signature} => 
                        Ok(signature),
                    _ => Err(SecurityModuleClientError::InvalidResponse),
                }
            })
    }
}
