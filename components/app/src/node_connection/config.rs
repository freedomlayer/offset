use futures::channel::mpsc;
use crypto::identity::{PublicKey, Signature};
use proto::app_server::messages::{AppToAppServer, RelayAddress, NamedRelayAddress};
use proto::index_server::messages::NamedIndexServerAddress;

pub struct AppConfig {
    sender: mpsc::Sender<AppToAppServer>,
}

impl AppConfig {
    pub fn new(sender: mpsc::Sender<AppToAppServer>) -> Self {
        AppConfig {
            sender,
        }
    }
    /*
    /// Set relay address to be used locally:
    SetRelays(NRA),
    /// Sending funds:
    RequestSendFunds(UserRequestSendFunds),
    ReceiptAck(ReceiptAck),
    /// Friend management:
    AddFriend(AddFriend<RA>),
    SetFriendRelays(SetFriendAddress<RA>),
    SetFriendName(SetFriendName),
    RemoveFriend(PublicKey),
    EnableFriend(PublicKey),
    DisableFriend(PublicKey),
    OpenFriend(PublicKey),
    CloseFriend(PublicKey),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    ResetFriendChannel(ResetFriendChannel),
    /// Request routes from one node to another:
    RequestRoutes(RequestRoutes),
    /// Manage index servers:
    AddIndexServer(AddIndexServer<ISA>),
    RemoveIndexServer(PublicKey),
    */

    pub async fn add_relay(named_relay_address: NamedRelayAddress) {
        unimplemented!();
    }

    pub async fn remove_relay(relay_public_key: PublicKey) {
        unimplemented!();
    }

    pub async fn add_friend(friend_public_key: PublicKey,
                            relays: Vec<RelayAddress>,
                            name: String,
                            balance: i128) {
        unimplemented!();
    }

    pub async fn set_friend_relays(relays: Vec<RelayAddress>) {
        unimplemented!();
    }

    pub async fn remove_friend(friend_public_key: PublicKey) {
        unimplemented!();
    }

    pub async fn enable_friend(friend_public_key: PublicKey) {
        unimplemented!();
    }

    pub async fn disable_friend(friend_public_key: PublicKey) {
        unimplemented!();
    }

    pub async fn open_friend(friend_public_key: PublicKey) {
        unimplemented!();
    }

    pub async fn close_friend(friend_public_key: PublicKey) {
        unimplemented!();
    }

    pub async fn set_friend_remote_max_debt(friend_public_key: PublicKey,
                                            remote_max_debt: u128) {
        unimplemented!();
    }

    pub async fn reset_friend_channel(friend_public_key: PublicKey,
                                      reset_token: Signature) {
        unimplemented!();
    }

    pub async fn add_index_server(named_index_server: NamedIndexServerAddress) {
        unimplemented!();
    }

    pub async fn remove_index_server(index_public_key: PublicKey) {
        unimplemented!();
    }
}

