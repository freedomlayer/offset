use serde::{Deserialize, Serialize};

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::PublicKey;
use proto::funder::messages::{Currency, Rate};

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

// TODO: Remove unused hint:
#[allow(unused)]
pub type FunderDbResult<T> = Result<T, FunderDbError>;

pub trait FunderDb<B> {
    fn local_public_key() -> PublicKey;
    fn get_friend_remote_relays(
        friend_public_key: PublicKey,
    ) -> FunderDbResult<Vec<RelayAddress<B>>>;
    fn set_friend_remote_relays(
        friend_public_key: PublicKey,
        remote_relays: Vec<RelayAddress<B>>,
    ) -> FunderDbResult<()>;
    fn get_friend_sent_local_relays(
        friend_public_key: PublicKey,
    ) -> FunderDbResult<SentLocalRelays<B>>;
    fn set_friend_sent_local_relays(
        friend_public_key: PublicKey,
        sent_local_relays: SentLocalRelays<B>,
    ) -> FunderDbResult<()>;
    fn get_friend_name(friend_public_key: PublicKey) -> FunderDbResult<String>;
    fn set_friend_name(friend_public_key: PublicKey, name: String) -> FunderDbResult<()>;
    fn get_friend_currency_config(
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> FunderDbResult<CurrencyConfig>;
    fn set_friend_currency_rate(
        friend_public_key: PublicKey,
        currency: Currency,
        rate: Rate,
    ) -> FunderDbResult<()>;
    fn set_friend_currency_remote_max_debt(
        friend_public_key: PublicKey,
        currency: Currency,
        remote_max_debt: u128,
    ) -> FunderDbResult<()>;
    fn set_friend_currency_is_open(
        friend_public_key: PublicKey,
        currency: Currency,
        is_open: bool,
    ) -> FunderDbResult<()>;
}
