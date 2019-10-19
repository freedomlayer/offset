use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

use common::multi_consumer::MultiConsumerClient;

use proto::crypto::{PublicKey, Signature, Uid};

use crypto::rand::{CryptoRandom, OffstSystemRandom, RandGen};

use proto::app_server::messages::{AppRequest, AppToAppServer, NamedRelayAddress, RelayAddress, OpenFriendCurrency, CloseFriendCurrency};
use proto::funder::messages::{
    AddFriend, Rate, ResetFriendChannel, SetFriendCurrencyRate, SetFriendRelays, SetFriendCurrencyMaxDebt, Currency
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
        let app_request_id = Uid::rand_gen(&self.rng);
        let to_app_server = AppToAppServer::new(app_request_id.clone(), app_request);

        // Start listening to done requests:
        let mut incoming_done_requests = self
            .done_app_requests_mc
            .request_stream()
            .await
            .map_err(|_| AppConfigError)?;

        // Send our request to offst node:
        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| AppConfigError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = incoming_done_requests.next().await {
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
        self.send_request(AppRequest::AddRelay(named_relay_address))
            .await
    }

    pub async fn remove_relay(
        &mut self,
        relay_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        self.send_request(AppRequest::RemoveRelay(relay_public_key))
            .await
    }

    pub async fn add_friend(
        &mut self,
        friend_public_key: PublicKey,
        relays: Vec<RelayAddress>,
        name: String,
    ) -> Result<(), AppConfigError> {
        let add_friend = AddFriend {
            friend_public_key,
            relays,
            name,
        };
        self.send_request(AppRequest::AddFriend(add_friend)).await
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
        self.send_request(AppRequest::SetFriendRelays(set_friend_relays))
            .await
    }

    pub async fn remove_friend(
        &mut self,
        friend_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        self.send_request(AppRequest::RemoveFriend(friend_public_key))
            .await
    }

    pub async fn enable_friend(
        &mut self,
        friend_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        self.send_request(AppRequest::EnableFriend(friend_public_key))
            .await
    }

    pub async fn disable_friend(
        &mut self,
        friend_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        self.send_request(AppRequest::DisableFriend(friend_public_key))
            .await
    }

    pub async fn open_friend_currency(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> Result<(), AppConfigError> {
        self.send_request(AppRequest::OpenFriendCurrency(OpenFriendCurrency {
            friend_public_key,
            currency,
        })).await
    }

    pub async fn close_friend_currency(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> Result<(), AppConfigError> {
        self.send_request(AppRequest::CloseFriendCurrency(CloseFriendCurrency {
            friend_public_key,
            currency,
        })).await
    }

    pub async fn set_friend_currency_max_debt(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        remote_max_debt: u128,
    ) -> Result<(), AppConfigError> {
        let set_friend_currency_max_debt = SetFriendCurrencyMaxDebt {
            friend_public_key,
            currency,
            remote_max_debt,
        };
        self.send_request(AppRequest::SetFriendCurrencyMaxDebt(
            set_friend_currency_max_debt,
        ))
        .await
    }

    pub async fn set_friend_currency_rate(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        rate: Rate,
    ) -> Result<(), AppConfigError> {
        let set_friend_currency_rate = SetFriendCurrencyRate {
            friend_public_key,
            currency,
            rate,
        };
        self.send_request(AppRequest::SetFriendCurrencyRate(set_friend_currency_rate))
            .await
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
        self.send_request(AppRequest::ResetFriendChannel(reset_friend_channel))
            .await
    }

    pub async fn add_index_server(
        &mut self,
        named_index_server: NamedIndexServerAddress,
    ) -> Result<(), AppConfigError> {
        self.send_request(AppRequest::AddIndexServer(named_index_server))
            .await
    }

    pub async fn remove_index_server(
        &mut self,
        index_public_key: PublicKey,
    ) -> Result<(), AppConfigError> {
        self.send_request(AppRequest::RemoveIndexServer(index_public_key))
            .await
    }
}
