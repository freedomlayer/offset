use serde::{Deserialize, Serialize};

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::PublicKey;
use proto::funder::messages::{Currency, Rate};

use crate::interfaces::types::{DbOpResult, DbOpSenderResult};

use futures::channel::mpsc;

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

// TODO: Remove this unused hint later:
#[allow(unused)]
pub enum FunderDbOp<B> {
    GetLocalPublicKey(DbOpSenderResult<PublicKey>),
    GetFriendIsEnabled(PublicKey, DbOpSenderResult<bool>),
    SetFriendIsEnabled(PublicKey, bool, DbOpSenderResult<()>),
    GetFriendRemoteRelays(PublicKey, DbOpSenderResult<Vec<RelayAddress<B>>>),
    SetFriendRemoteRelays(PublicKey, Vec<RelayAddress<B>>, DbOpSenderResult<()>),
    GetFriendSentLocalRelays(PublicKey, DbOpSenderResult<Vec<RelayAddress<B>>>),
    SetFriendSentLocalRelays(PublicKey, Vec<RelayAddress<B>>, DbOpSenderResult<()>),
    GetFriendName(PublicKey, DbOpSenderResult<String>),
    SetFriendName(PublicKey, String, DbOpSenderResult<()>),
    GetCurrencyConfig(PublicKey, DbOpSenderResult<Currency>),
    SetFriendCurrencyRate(PublicKey, Currency, Rate, DbOpSenderResult<()>),
    SetFriendCurrencyRemoteMaxDebt(PublicKey, Currency, u128, DbOpSenderResult<()>),
    SetFriendCurrencyIsOpen(PublicKey, Currency, bool, DbOpSenderResult<()>),
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
    ) -> DbOpResult<Vec<RelayAddress<B>>> {
        unimplemented!();
    }
    pub async fn set_friend_remote_relays(
        friend_public_key: PublicKey,
        remote_relays: Vec<RelayAddress<B>>,
    ) -> DbOpResult<()> {
        unimplemented!();
    }
    pub async fn get_friend_sent_local_relays(
        friend_public_key: PublicKey,
    ) -> DbOpResult<SentLocalRelays<B>> {
        unimplemented!();
    }
    pub async fn set_friend_sent_local_relays(
        friend_public_key: PublicKey,
        sent_local_relays: SentLocalRelays<B>,
    ) -> DbOpResult<()> {
        unimplemented!();
    }
    pub async fn get_friend_name(friend_public_key: PublicKey) -> DbOpResult<String> {
        unimplemented!();
    }
    pub async fn set_friend_name(friend_public_key: PublicKey, name: String) -> DbOpResult<()> {
        unimplemented!();
    }
    pub async fn get_friend_currency_config(
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> DbOpResult<CurrencyConfig> {
        unimplemented!();
    }
    pub async fn set_friend_currency_rate(
        friend_public_key: PublicKey,
        currency: Currency,
        rate: Rate,
    ) -> DbOpResult<()> {
        unimplemented!();
    }
    pub async fn set_friend_currency_remote_max_debt(
        friend_public_key: PublicKey,
        currency: Currency,
        remote_max_debt: u128,
    ) -> DbOpResult<()> {
        unimplemented!();
    }

    async fn set_friend_currency_is_open(
        friend_public_key: PublicKey,
        currency: Currency,
        is_open: bool,
    ) -> DbOpResult<()> {
        unimplemented!();
    }
}
