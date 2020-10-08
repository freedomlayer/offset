use serde::{Deserialize, Serialize};

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::PublicKey;
use proto::funder::messages::{Currency, Rate};

use futures::channel::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SentLocalRelays<B> {
    NeverSent,
    Transition(Vec<NamedRelayAddress<B>>, Vec<NamedRelayAddress<B>>), // (last sent, before last sent)
    LastSent(Vec<NamedRelayAddress<B>>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyConfig {
    /// Rate of forwarding transactions that arrived from this friend to any other friend
    /// for a certain currency.
    pub rate: Rate,
    /// Credit frame for the remote side (Set by the user of this node)
    /// The remote side does not know this value.
    pub remote_max_debt: u128,
    /// Can new requests be sent through the mutual credit with this friend?
    pub is_open: bool,
}

pub enum FunderDbError {}

pub type FunderDbResult<T> = Result<T, FunderDbError>;
pub type SenderResult<T> = oneshot::Sender<FunderDbResult<T>>;

// TODO: Remove this unused hint later:
#[allow(unused)]
pub enum FunderDbOp<B> {
    GetLocalPublicKey(oneshot::Sender<FunderDbResult<PublicKey>>),
    GetFriendIsEnabled(PublicKey, SenderResult<bool>),
    SetFriendIsEnabled(PublicKey, bool, SenderResult<()>),
    GetFriendRemoteRelays(PublicKey, SenderResult<Vec<RelayAddress<B>>>),
    SetFriendRemoteRelays(PublicKey, Vec<RelayAddress<B>>, SenderResult<()>),
    GetFriendSentLocalRelays(PublicKey, SenderResult<Vec<RelayAddress<B>>>),
    SetFriendSentLocalRelays(PublicKey, Vec<RelayAddress<B>>, SenderResult<()>),
    GetFriendName(PublicKey, SenderResult<String>),
    SetFriendName(PublicKey, String, SenderResult<()>),
    GetCurrencyConfig(PublicKey, SenderResult<Currency>),
    SetFriendCurrencyRate(PublicKey, Currency, Rate, SenderResult<()>),
    SetFriendCurrencyRemoteMaxDebt(PublicKey, Currency, u128, SenderResult<()>),
    SetFriendCurrencyIsOpen(PublicKey, Currency, bool, SenderResult<()>),
}

// TODO: Remove this unused hint later:
#[allow(unused)]
pub struct FunderDbClient<B> {
    sender: mpsc::Sender<FunderDbOp<B>>,
}

// TODO: Remove this unused hint later:
#[allow(unused)]
impl<B> FunderDbClient<B> {
    pub async fn local_public_key() -> PublicKey {
        unimplemented!();
    }
    pub async fn get_friend_remote_relays(
        friend_public_key: PublicKey,
    ) -> FunderDbResult<Vec<RelayAddress<B>>> {
        unimplemented!();
    }
    pub async fn set_friend_remote_relays(
        friend_public_key: PublicKey,
        remote_relays: Vec<RelayAddress<B>>,
    ) -> FunderDbResult<()> {
        unimplemented!();
    }
    pub async fn get_friend_sent_local_relays(
        friend_public_key: PublicKey,
    ) -> FunderDbResult<SentLocalRelays<B>> {
        unimplemented!();
    }
    pub async fn set_friend_sent_local_relays(
        friend_public_key: PublicKey,
        sent_local_relays: SentLocalRelays<B>,
    ) -> FunderDbResult<()> {
        unimplemented!();
    }
    pub async fn get_friend_name(friend_public_key: PublicKey) -> FunderDbResult<String> {
        unimplemented!();
    }
    pub async fn set_friend_name(friend_public_key: PublicKey, name: String) -> FunderDbResult<()> {
        unimplemented!();
    }
    pub async fn get_friend_currency_config(
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> FunderDbResult<CurrencyConfig> {
        unimplemented!();
    }
    pub async fn set_friend_currency_rate(
        friend_public_key: PublicKey,
        currency: Currency,
        rate: Rate,
    ) -> FunderDbResult<()> {
        unimplemented!();
    }
    pub async fn set_friend_currency_remote_max_debt(
        friend_public_key: PublicKey,
        currency: Currency,
        remote_max_debt: u128,
    ) -> FunderDbResult<()> {
        unimplemented!();
    }

    async fn set_friend_currency_is_open(
        friend_public_key: PublicKey,
        currency: Currency,
        is_open: bool,
    ) -> FunderDbResult<()> {
        unimplemented!();
    }
}
