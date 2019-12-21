use proto::crypto::{PublicKey, Signature};

use proto::app_server::messages::{
    AppRequest, CloseFriendCurrency, NamedRelayAddress, OpenFriendCurrency, RelayAddress,
};
use proto::funder::messages::{
    AddFriend, Currency, Rate, RemoveFriendCurrency, ResetFriendChannel, SetFriendCurrencyMaxDebt,
    SetFriendCurrencyRate, SetFriendName, SetFriendRelays,
};
use proto::index_server::messages::NamedIndexServerAddress;

pub fn add_relay(named_relay_address: NamedRelayAddress) -> AppRequest {
    AppRequest::AddRelay(named_relay_address)
}

pub fn remove_relay(relay_public_key: PublicKey) -> AppRequest {
    AppRequest::RemoveRelay(relay_public_key)
}

pub fn add_friend(
    friend_public_key: PublicKey,
    relays: Vec<RelayAddress>,
    name: String,
) -> AppRequest {
    let add_friend = AddFriend {
        friend_public_key,
        relays,
        name,
    };
    AppRequest::AddFriend(add_friend)
}

pub fn set_friend_relays(friend_public_key: PublicKey, relays: Vec<RelayAddress>) -> AppRequest {
    let set_friend_relays = SetFriendRelays {
        friend_public_key,
        relays,
    };
    AppRequest::SetFriendRelays(set_friend_relays)
}

pub fn set_friend_name(friend_public_key: PublicKey, name: String) -> AppRequest {
    let set_friend_name = SetFriendName {
        friend_public_key,
        name,
    };
    AppRequest::SetFriendName(set_friend_name)
}

pub fn remove_friend(friend_public_key: PublicKey) -> AppRequest {
    AppRequest::RemoveFriend(friend_public_key)
}

pub fn enable_friend(friend_public_key: PublicKey) -> AppRequest {
    AppRequest::EnableFriend(friend_public_key)
}

pub fn disable_friend(friend_public_key: PublicKey) -> AppRequest {
    AppRequest::DisableFriend(friend_public_key)
}

pub fn remove_friend_currency(friend_public_key: PublicKey, currency: Currency) -> AppRequest {
    AppRequest::RemoveFriendCurrency(RemoveFriendCurrency {
        friend_public_key,
        currency,
    })
}

pub fn open_friend_currency(friend_public_key: PublicKey, currency: Currency) -> AppRequest {
    AppRequest::OpenFriendCurrency(OpenFriendCurrency {
        friend_public_key,
        currency,
    })
}

pub fn close_friend_currency(friend_public_key: PublicKey, currency: Currency) -> AppRequest {
    AppRequest::CloseFriendCurrency(CloseFriendCurrency {
        friend_public_key,
        currency,
    })
}

pub fn set_friend_currency_max_debt(
    friend_public_key: PublicKey,
    currency: Currency,
    remote_max_debt: u128,
) -> AppRequest {
    let set_friend_currency_max_debt = SetFriendCurrencyMaxDebt {
        friend_public_key,
        currency,
        remote_max_debt,
    };
    AppRequest::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt)
}

pub fn set_friend_currency_rate(
    friend_public_key: PublicKey,
    currency: Currency,
    rate: Rate,
) -> AppRequest {
    let set_friend_currency_rate = SetFriendCurrencyRate {
        friend_public_key,
        currency,
        rate,
    };
    AppRequest::SetFriendCurrencyRate(set_friend_currency_rate)
}

pub fn reset_friend_channel(friend_public_key: PublicKey, reset_token: Signature) -> AppRequest {
    // TODO: Check if a reset confusion attack is possible here.
    // Maybe we (locally) should be the ones generating the reset token.
    // What happens if the remote side sends two consecutive reset requests with different
    // amounts?
    let reset_friend_channel = ResetFriendChannel {
        friend_public_key,
        reset_token,
    };
    AppRequest::ResetFriendChannel(reset_friend_channel)
}

pub fn add_index_server(named_index_server: NamedIndexServerAddress) -> AppRequest {
    AppRequest::AddIndexServer(named_index_server)
}

pub fn remove_index_server(index_public_key: PublicKey) -> AppRequest {
    AppRequest::RemoveIndexServer(index_public_key)
}
