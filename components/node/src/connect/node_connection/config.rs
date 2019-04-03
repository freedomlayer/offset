use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

use common::multi_consumer::MultiConsumerClient;

use crypto::crypto_rand::{CryptoRandom, OffstSystemRandom};
use crypto::identity::{PublicKey, Signature};
use crypto::uid::Uid;

use proto::app_server::messages::{AppRequest, AppToAppServer, NamedRelayAddress, RelayAddress};
use proto::funder::messages::{
    AddFriend, ResetFriendChannel, SetFriendRelays, SetFriendRemoteMaxDebt,
};
use proto::index_server::messages::NamedIndexServerAddress;

#[derive(Debug)]
pub struct AppConfigError;

#[derive(Clone)]
pub struct AppConfig<R = OffstSystemRandom> {
    sender: mpsc::Sender<AppToAppServer>,
    done_app_requests_mc: MultiConsumerClient<Uid>,
    rng: R,
}

impl<R> AppConfig<R>
where
    R: CryptoRandom,
{
    pub(super) fn new(
        sender: mpsc::Sender<AppToAppServer>,
        done_app_requests_mc: MultiConsumerClient<Uid>,
        rng: R,
    ) -> Self {
        AppConfig {
            sender,
            done_app_requests_mc,
            rng,
        }
    }

    async fn send_request(&mut self, app_request: AppRequest) -> Result<(), AppConfigError> {
        // Randomly generate a new app_request_id:
        let app_request_id = Uid::new(&self.rng);
        let to_app_server = AppToAppServer::new(app_request_id.clone(), app_request);

        // Start listening to done requests:
        let mut incoming_done_requests =
            await!(self.done_app_requests_mc.request_stream()).map_err(|_| AppConfigError)?;

        // Send our request to offst node:
        await!(self.sender.send(to_app_server)).map_err(|_| AppConfigError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = await!(incoming_done_requests.next()) {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }
        Err(AppConfigError)
    }

    pub async fn add_relay(
        &mut self,
        named_relay_address: NamedRelayAddress,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::AddRelay(named_relay_address)))
    }

    pub async fn remove_relay(
        &mut self,
        relay_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::RemoveRelay(relay_public_key)))
    }

    pub async fn add_friend(
        &mut self,
        friend_public_key: PublicKey,
        relays: Vec<RelayAddress>,
        name: String,
        balance: i128,
    ) -> Result<(), AppConfigError> {
        let add_friend = AddFriend {
            friend_public_key,
            relays,
            name,
            balance,
        };
        await!(self.send_request(AppRequest::AddFriend(add_friend)))
    }

    pub async fn set_friend_relays(
        &mut self,
        friend_public_key: PublicKey,
        relays: Vec<RelayAddress>,
    ) -> Result<(), AppConfigError> {
        let set_friend_relays = SetFriendRelays {
            friend_public_key,
            relays,
        };
        await!(self.send_request(AppRequest::SetFriendRelays(set_friend_relays)))
    }

    pub async fn remove_friend(
        &mut self,
        friend_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::RemoveFriend(friend_public_key)))
    }

    pub async fn enable_friend(
        &mut self,
        friend_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::EnableFriend(friend_public_key)))
    }

    pub async fn disable_friend(
        &mut self,
        friend_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::DisableFriend(friend_public_key)))
    }

    pub async fn open_friend(
        &mut self,
        friend_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::OpenFriend(friend_public_key)))
    }

    pub async fn close_friend(
        &mut self,
        friend_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::CloseFriend(friend_public_key)))
    }

    pub async fn set_friend_remote_max_debt(
        &mut self,
        friend_public_key: PublicKey,
        remote_max_debt: u128,
    ) -> Result<(), AppConfigError> {
        let set_friend_remote_max_debt = SetFriendRemoteMaxDebt {
            friend_public_key,
            remote_max_debt,
        };
        await!(self.send_request(AppRequest::SetFriendRemoteMaxDebt(
            set_friend_remote_max_debt
        )))
    }

    pub async fn reset_friend_channel(
        &mut self,
        friend_public_key: PublicKey,
        reset_token: Signature,
    ) -> Result<(), AppConfigError> {
        // TODO: Check if a reset confusion attack is possible here.
        // Maybe we (locally) should be the ones generating the reset token.
        // What happens if the remote side sends two consecutive reset requests with different
        // amounts?
        let reset_friend_channel = ResetFriendChannel {
            friend_public_key,
            reset_token,
        };
        await!(self.send_request(AppRequest::ResetFriendChannel(reset_friend_channel)))
    }

    pub async fn add_index_server(
        &mut self,
        named_index_server: NamedIndexServerAddress,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::AddIndexServer(named_index_server)))
    }

    pub async fn remove_index_server(
        &mut self,
        index_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        await!(self.send_request(AppRequest::RemoveIndexServer(index_public_key)))
    }
}
