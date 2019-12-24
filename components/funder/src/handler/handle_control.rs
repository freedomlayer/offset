use std::fmt::Debug;

use signature::canonical::CanonicalSerialize;

use crypto::hash_lock::HashLock;
use crypto::rand::{CryptoRandom, RandGen};

use proto::crypto::{InvoiceId, PaymentId, PlainLock, PublicKey, Uid};

use crate::friend::{BackwardsOp, ChannelStatus, CurrencyConfig, FriendMutation};
use crate::state::{FunderMutation, NewTransactions, Payment, PaymentStage};

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::funder::messages::{
    AckClosePayment, AddFriend, AddInvoice, ChannelerUpdateFriend, CollectSendFundsOp, Commit,
    CreatePayment, CreateTransaction, FriendStatus, FunderControl, FunderOutgoingControl,
    PaymentStatus, PaymentStatusSuccess, RemoveFriend, RemoveFriendCurrency, RequestResult,
    RequestSendFundsOp, ResetFriendChannel, ResponseClosePayment, SetFriendCurrencyMaxDebt,
    SetFriendCurrencyRate, SetFriendCurrencyRequestsStatus, SetFriendName, SetFriendRelays,
    SetFriendStatus, TransactionResult,
};
use signature::verify::verify_commit;

use crate::ephemeral::Ephemeral;
use crate::handler::canceler::{
    cancel_local_pending_transactions, cancel_pending_requests, reply_with_cancel, CurrencyChoice,
};
use crate::handler::prepare::prepare_commit;
use crate::handler::state_wrap::{MutableEphemeral, MutableFunderState};
use crate::handler::types::SendCommands;
use crate::handler::utils::{find_local_pending_transaction, find_request_origin, is_friend_ready};

use crate::types::ChannelerConfig;

#[derive(Debug)]
pub enum HandleControlError {
    FriendDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
    NotFirstInRoute,
    PaymentDestNotLastInRoute,
    InvalidRoute,
    RequestAlreadyInProgress,
    PendingUserRequestsFull,
    FriendNotReady,
    MaxNodeRelaysReached,
    PaymentAlreadyOpen,
    OpenPaymentNotFound,
    NewTransactionsNotAllowed,
    PaymentDoesNotExist,
    AckStateInvalid,
    AckMismatch,
    InvoiceAlreadyExists,
    InvoiceDoesNotExist,
    InvalidCommit,
    FriendCurrencyDoesNotExist,
    CanNotRemoveActiveCurrency,
    CurrencyNotConfigured,
}

fn control_set_friend_currency_max_debt<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    set_friend_currency_max_debt: SetFriendCurrencyMaxDebt,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Make sure that friend exists:
    let friend = m_state
        .state()
        .friends
        .get(&set_friend_currency_max_debt.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    // If the newly proposed rate is the same as the old one, we do nothing:
    let mut new_currency_config = if let Some(currency_config) = friend
        .currency_configs
        .get(&set_friend_currency_max_debt.currency)
    {
        if currency_config.remote_max_debt == set_friend_currency_max_debt.remote_max_debt {
            return Ok(());
        }
        currency_config.clone()
    } else {
        CurrencyConfig::new()
    };

    new_currency_config.remote_max_debt = set_friend_currency_max_debt.remote_max_debt;

    let friend_mutation = FriendMutation::UpdateCurrencyConfig((
        set_friend_currency_max_debt.currency,
        new_currency_config,
    ));
    let funder_mutation = FunderMutation::FriendMutation((
        set_friend_currency_max_debt.friend_public_key.clone(),
        friend_mutation,
    ));
    m_state.mutate(funder_mutation);

    send_commands.set_try_send(&set_friend_currency_max_debt.friend_public_key);
    Ok(())
}

fn control_reset_friend_channel<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    reset_friend_channel: ResetFriendChannel,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    let friend = m_state
        .state()
        .friends
        .get(&reset_friend_channel.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    match &friend.channel_status {
        ChannelStatus::Consistent(_) => Err(HandleControlError::NotInvitedToReset),
        ChannelStatus::Inconsistent(channel_inconsistent) => {
            match &channel_inconsistent.opt_remote_reset_terms {
                None => Err(HandleControlError::NotInvitedToReset),
                Some(remote_reset_terms) => {
                    if remote_reset_terms.reset_token != reset_friend_channel.reset_token {
                        Err(HandleControlError::ResetTokenMismatch)
                    } else {
                        Ok(())
                    }
                }
            }
        }
    }?;

    // We don't have the ability to sign here, therefore we defer the creation
    // of the local reset outgoing move token to the sender.
    send_commands.set_local_reset(&reset_friend_channel.friend_public_key);

    Ok(())
}

fn enable_friend<B>(
    m_state: &mut MutableFunderState<B>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    friend_public_key: &PublicKey,
    friend_relays: &[RelayAddress<B>],
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Notify Channeler:
    let channeler_add_friend = ChannelerUpdateFriend {
        friend_public_key: friend_public_key.clone(),
        friend_relays: friend_relays.to_vec(),
        local_relays: friend.sent_local_relays.to_vec(),
    };
    let channeler_config = ChannelerConfig::UpdateFriend(channeler_add_friend);
    outgoing_channeler_config.push(channeler_config);
}

fn disable_friend<B, R>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    rng: &R,
    friend_public_key: &PublicKey,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Cancel all pending requests to this friend:
    cancel_pending_requests(
        m_state,
        send_commands,
        outgoing_control,
        rng,
        friend_public_key,
        &CurrencyChoice::All,
    );

    // Notify Channeler:
    let channeler_config = ChannelerConfig::RemoveFriend(friend_public_key.clone());
    outgoing_channeler_config.push(channeler_config);
}

fn control_add_relay<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    max_node_relays: usize,
    named_relay_address: NamedRelayAddress<B>,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // We can't have more than `max_node_relays` relays
    if m_state.state().relays.len() >= max_node_relays {
        return Err(HandleControlError::MaxNodeRelaysReached);
    }

    let funder_mutation = FunderMutation::AddRelay(named_relay_address);
    m_state.mutate(funder_mutation);

    let relays = m_state
        .state()
        .relays
        .iter()
        .cloned()
        .map(RelayAddress::from)
        .collect::<Vec<_>>();

    // Notify Channeler about relay address change:
    let channeler_config = ChannelerConfig::SetRelays(relays);
    outgoing_channeler_config.push(channeler_config);

    // We might need to update all friends about the address change:
    let friend_public_keys = m_state.state().friends.keys().cloned().collect::<Vec<_>>();

    for friend_public_key in &friend_public_keys {
        send_commands.set_try_send(friend_public_key);
    }
    Ok(())
}

fn control_remove_relay<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    public_key: PublicKey,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    let funder_mutation = FunderMutation::RemoveRelay(public_key);
    m_state.mutate(funder_mutation);

    let relays = m_state
        .state()
        .relays
        .iter()
        .cloned()
        .map(RelayAddress::from)
        .collect::<Vec<_>>();

    // Notify Channeler about relay address change:
    let channeler_config = ChannelerConfig::SetRelays(relays);
    outgoing_channeler_config.push(channeler_config);

    // We might need to update all friends about the address change:
    let friend_public_keys = m_state.state().friends.keys().cloned().collect::<Vec<_>>();

    for friend_public_key in &friend_public_keys {
        send_commands.set_try_send(friend_public_key);
    }
}

fn control_add_friend<B>(m_state: &mut MutableFunderState<B>, add_friend: AddFriend<B>)
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    let funder_mutation = FunderMutation::AddFriend(add_friend);
    m_state.mutate(funder_mutation);
}

/// This is a violent operation, as it removes all the known state with the remote friend.
/// An inconsistency will occur if the friend is added again.
fn control_remove_friend<B, R>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    rng: &R,
    remove_friend: RemoveFriend,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Make sure that friend exists:
    let _friend = m_state
        .state()
        .friends
        .get(&remove_friend.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    disable_friend(
        m_state,
        send_commands,
        outgoing_control,
        outgoing_channeler_config,
        rng,
        &remove_friend.friend_public_key,
    );

    cancel_local_pending_transactions(
        m_state,
        send_commands,
        outgoing_control,
        rng,
        &remove_friend.friend_public_key,
    );

    let funder_mutation = FunderMutation::RemoveFriend(remove_friend.friend_public_key.clone());
    m_state.mutate(funder_mutation);

    Ok(())
}

fn control_set_friend_status<B, R>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    rng: &R,
    set_friend_status: SetFriendStatus,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Make sure that friend exists:
    let _ = m_state
        .state()
        .friends
        .get(&set_friend_status.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    let friend_mutation = FriendMutation::SetStatus(set_friend_status.status.clone());
    let funder_mutation = FunderMutation::FriendMutation((
        set_friend_status.friend_public_key.clone(),
        friend_mutation,
    ));
    m_state.mutate(funder_mutation);

    let friend = m_state
        .state()
        .friends
        .get(&set_friend_status.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    let friend_public_key = &set_friend_status.friend_public_key;
    let friend_address = friend.remote_relays.clone();

    match set_friend_status.status {
        FriendStatus::Enabled => enable_friend(
            m_state,
            outgoing_channeler_config,
            friend_public_key,
            &friend_address,
        ),
        FriendStatus::Disabled => disable_friend(
            m_state,
            send_commands,
            outgoing_control,
            outgoing_channeler_config,
            rng,
            &friend_public_key,
        ),
    };

    Ok(())
}

fn control_set_friend_currency_requests_status<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    set_friend_currency_requests_status: SetFriendCurrencyRequestsStatus,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Make sure that friend exists:
    let friend = m_state
        .state()
        .friends
        .get(&set_friend_currency_requests_status.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    // If the newly proposed rate is the same as the old one, we do nothing:
    let mut new_currency_config = if let Some(currency_config) = friend
        .currency_configs
        .get(&set_friend_currency_requests_status.currency)
    {
        if currency_config.wanted_local_requests_status
            == set_friend_currency_requests_status.status
        {
            return Ok(());
        }
        currency_config.clone()
    } else {
        CurrencyConfig::new()
    };

    new_currency_config.wanted_local_requests_status = set_friend_currency_requests_status.status;

    let friend_mutation = FriendMutation::UpdateCurrencyConfig((
        set_friend_currency_requests_status.currency,
        new_currency_config,
    ));
    let funder_mutation = FunderMutation::FriendMutation((
        set_friend_currency_requests_status
            .friend_public_key
            .clone(),
        friend_mutation,
    ));
    m_state.mutate(funder_mutation);

    send_commands.set_try_send(&set_friend_currency_requests_status.friend_public_key);
    Ok(())
}

fn control_set_friend_relays<B>(
    m_state: &mut MutableFunderState<B>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    set_friend_relays: SetFriendRelays<B>,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Make sure that friend exists:
    let friend = m_state
        .state()
        .friends
        .get(&set_friend_relays.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;
    let friend_status = friend.status.clone();

    // If the newly proposed address is the same as the old one,
    // we do nothing:
    if set_friend_relays.relays == friend.remote_relays {
        return Ok(());
    }

    let local_relays = friend.sent_local_relays.to_vec();

    let friend_mutation = FriendMutation::SetRemoteRelays(set_friend_relays.relays.clone());
    let funder_mutation = FunderMutation::FriendMutation((
        set_friend_relays.friend_public_key.clone(),
        friend_mutation,
    ));
    m_state.mutate(funder_mutation);

    if let FriendStatus::Enabled = friend_status {
        // Notify Channeler to change the friend's address:
        let update_friend = ChannelerUpdateFriend {
            friend_public_key: set_friend_relays.friend_public_key.clone(),
            friend_relays: set_friend_relays.relays.clone(),
            local_relays,
        };
        let channeler_config = ChannelerConfig::UpdateFriend(update_friend);
        outgoing_channeler_config.push(channeler_config);
    }

    Ok(())
}

fn control_set_friend_name<B>(
    m_state: &mut MutableFunderState<B>,
    set_friend_name: SetFriendName,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Make sure that friend exists:
    let friend = m_state
        .state()
        .friends
        .get(&set_friend_name.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    // If the newly proposed name is the same as the old one, we do nothing:
    if friend.name == set_friend_name.name {
        return Ok(());
    }

    let friend_mutation = FriendMutation::SetName(set_friend_name.name);
    let funder_mutation = FunderMutation::FriendMutation((
        set_friend_name.friend_public_key.clone(),
        friend_mutation,
    ));
    m_state.mutate(funder_mutation);

    Ok(())
}

fn control_set_friend_currency_rate<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    set_friend_currency_rate: SetFriendCurrencyRate,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Make sure that friend exists:
    let friend = m_state
        .state()
        .friends
        .get(&set_friend_currency_rate.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    // If the newly proposed rate is the same as the old one, we do nothing:
    let mut new_currency_config = if let Some(currency_config) = friend
        .currency_configs
        .get(&set_friend_currency_rate.currency)
    {
        if currency_config.rate == set_friend_currency_rate.rate {
            return Ok(());
        }
        currency_config.clone()
    } else {
        // A new local currency was added, we need to report to remote side:
        send_commands.set_try_send(&set_friend_currency_rate.friend_public_key);
        CurrencyConfig::new()
    };

    new_currency_config.rate = set_friend_currency_rate.rate;

    let friend_mutation = FriendMutation::UpdateCurrencyConfig((
        set_friend_currency_rate.currency,
        new_currency_config,
    ));
    let funder_mutation = FunderMutation::FriendMutation((
        set_friend_currency_rate.friend_public_key.clone(),
        friend_mutation,
    ));
    m_state.mutate(funder_mutation);

    Ok(())
}

fn control_remove_friend_currency<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    remove_friend_currency: RemoveFriendCurrency,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Make sure that friend exists:
    let friend = m_state
        .state()
        .friends
        .get(&remove_friend_currency.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    if !friend
        .currency_configs
        .contains_key(&remove_friend_currency.currency)
    {
        return Err(HandleControlError::CurrencyNotConfigured);
    }

    if let ChannelStatus::Consistent(channel_consistent) = &friend.channel_status {
        if channel_consistent
            .token_channel
            .get_active_currencies()
            .calc_active()
            .contains(&remove_friend_currency.currency)
        {
            return Err(HandleControlError::CanNotRemoveActiveCurrency);
        }
    }

    let friend_mutation =
        FriendMutation::RemoveCurrencyConfig(remove_friend_currency.currency.clone());
    let funder_mutation = FunderMutation::FriendMutation((
        remove_friend_currency.friend_public_key.clone(),
        friend_mutation,
    ));
    m_state.mutate(funder_mutation);
    send_commands.set_try_send(&remove_friend_currency.friend_public_key);

    Ok(())
}

fn control_create_payment<B, R>(
    m_state: &mut MutableFunderState<B>,
    rng: &R,
    create_payment: CreatePayment,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Make sure that a payment with the same payment_id doesn't exist:
    if m_state
        .state()
        .payments
        .contains_key(&create_payment.payment_id)
    {
        return Err(HandleControlError::PaymentAlreadyOpen);
    }

    // TODO: Possibly check:
    // - dest_public_key exists
    // - currency is active for this friend

    let stage = PaymentStage::NewTransactions(NewTransactions {
        num_transactions: 0,
        invoice_id: create_payment.invoice_id.clone(),
        currency: create_payment.currency.clone(),
        total_dest_payment: create_payment.total_dest_payment,
        dest_public_key: create_payment.dest_public_key.clone(),
    });

    let payment = Payment {
        src_plain_lock: PlainLock::rand_gen(rng),
        stage,
    };

    // Add a new payment entry:
    let m_mutation = FunderMutation::UpdatePayment((create_payment.payment_id, payment));

    m_state.mutate(m_mutation);
    Ok(())
}

fn control_create_transaction_inner<B>(
    m_state: &mut MutableFunderState<B>,
    ephemeral: &Ephemeral,
    send_commands: &mut SendCommands,
    max_pending_user_requests: usize,
    create_transaction: CreateTransaction,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Make sure that a corresponding payment is open
    let payment = m_state
        .state()
        .payments
        .get(&create_transaction.payment_id)
        .ok_or(HandleControlError::OpenPaymentNotFound)?;

    let src_plain_lock = payment.src_plain_lock.clone();

    let new_transactions = if let PaymentStage::NewTransactions(new_transactions) = &payment.stage {
        new_transactions.clone()
    } else {
        return Err(HandleControlError::NewTransactionsNotAllowed);
    };

    let route = &create_transaction.route;

    // We have to be the first on the route:
    match route.public_keys.first() {
        Some(first) if *first == m_state.state().local_public_key => Ok(()),
        _ => Err(HandleControlError::NotFirstInRoute),
    }?;

    match route.public_keys.last() {
        Some(last) if *last == new_transactions.dest_public_key => Ok(()),
        _ => Err(HandleControlError::PaymentDestNotLastInRoute),
    }?;

    // We want to have at least two public keys on the route (source and destination).
    // We also want that the public keys on the route are unique.
    if !route.is_valid() {
        return Err(HandleControlError::InvalidRoute);
    }
    let friend_public_key = route.public_keys[1].clone();

    let friend = match m_state.state().friends.get(&friend_public_key) {
        Some(friend) => Ok(friend),
        None => Err(HandleControlError::FriendDoesNotExist),
    }?;

    let currency = new_transactions.currency.clone();

    if !is_friend_ready(m_state.state(), ephemeral, &friend_public_key, &currency) {
        return Err(HandleControlError::FriendNotReady);
    }

    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Inconsistent(_) => {
            // It is impossible that the Channel is Inconsistent, because we know that this friend is
            // in ready state:
            unreachable!();
        }
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
    };

    // TODO: Do we actually need to check it here? Maybe it should be checked later, inside
    // MutualCredit?
    //
    // If this transaction is already in progress, we do nothing:
    // Check if there is already a pending user transaction with the same request_id:
    for (currency0, user_request) in &channel_consistent.pending_user_requests {
        if (*currency0 == currency) && (create_transaction.request_id == user_request.request_id) {
            return Err(HandleControlError::RequestAlreadyInProgress);
        }
    }

    // Check if there is an ongoing request with the same request_id with this specific friend:
    if channel_consistent
        .token_channel
        .get_mutual_credits()
        .get(&currency)
        .ok_or(HandleControlError::FriendCurrencyDoesNotExist)?
        .state()
        .pending_transactions
        .local
        .contains_key(&create_transaction.request_id)
    {
        return Err(HandleControlError::RequestAlreadyInProgress);
    }

    // Check if we have room to push this message:
    if channel_consistent.pending_user_requests.len() >= max_pending_user_requests {
        return Err(HandleControlError::PendingUserRequestsFull);
    }

    // Keep PlainLock:
    let funder_mutation = FunderMutation::AddTransaction((
        create_transaction.request_id.clone(),
        create_transaction.payment_id.clone(),
    ));
    m_state.mutate(funder_mutation);

    // Update OpenPayment (Increase open transactions count):
    let mut updated_new_transactions = new_transactions.clone();
    updated_new_transactions.num_transactions =
        new_transactions.num_transactions.checked_add(1).unwrap();

    let payment = Payment {
        src_plain_lock: src_plain_lock.clone(),
        stage: PaymentStage::NewTransactions(updated_new_transactions),
    };

    let funder_mutation = FunderMutation::UpdatePayment((create_transaction.payment_id, payment));
    m_state.mutate(funder_mutation);

    let mut route_tail = create_transaction.route;
    // Remove ourselves from the remaining route:
    route_tail.public_keys.remove(0);
    // Remove next node from the route:
    route_tail.public_keys.remove(0);

    // Push the request:
    let request_send_funds = RequestSendFundsOp {
        request_id: create_transaction.request_id,
        src_hashed_lock: src_plain_lock.hash_lock(),
        route: route_tail,
        dest_payment: create_transaction.dest_payment,
        total_dest_payment: new_transactions.total_dest_payment,
        invoice_id: new_transactions.invoice_id,
        left_fees: create_transaction.fees,
    };

    let friend_mutation =
        FriendMutation::PushBackPendingUserRequest((currency, request_send_funds));
    let funder_mutation =
        FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    // Signal the sender to attempt to send:
    send_commands.set_try_send(&friend_public_key);
    Ok(())
}

fn control_create_transaction<B>(
    m_state: &mut MutableFunderState<B>,
    ephemeral: &Ephemeral,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    send_commands: &mut SendCommands,
    max_pending_user_requests: usize,
    create_transaction: CreateTransaction,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Find the corresponding payment:
    let payment = m_state
        .state()
        .payments
        .get(&create_transaction.payment_id)
        .ok_or(HandleControlError::OpenPaymentNotFound)?;

    let new_transactions = if let PaymentStage::NewTransactions(new_transactions) = &payment.stage {
        new_transactions.clone()
    } else {
        return Err(HandleControlError::NewTransactionsNotAllowed);
    };

    // If we already have this transaction:
    // - If we have a ready response, we return a Commit message.
    // - Else, we do nothing.
    //
    // This could happen if the user has disconnected before managing to obtain the
    // `RequestResult::Success` message.
    if let Some(open_transaction) = m_state
        .state()
        .open_transactions
        .get(&create_transaction.request_id)
    {
        if let (Some(response_send_funds), Some(pending_transaction)) = (
            &open_transaction.opt_response,
            find_local_pending_transaction(
                m_state.state(),
                &new_transactions.currency,
                &create_transaction.request_id,
            ),
        ) {
            let transaction_result = if response_send_funds.is_complete {
                let commit = prepare_commit(
                    new_transactions.currency.clone(),
                    response_send_funds,
                    pending_transaction,
                    payment.src_plain_lock.clone(),
                );

                TransactionResult {
                    request_id: response_send_funds.request_id.clone(),
                    result: RequestResult::Complete(commit),
                }
            } else {
                TransactionResult {
                    request_id: response_send_funds.request_id.clone(),
                    result: RequestResult::Success,
                }
            };
            outgoing_control.push(FunderOutgoingControl::TransactionResult(transaction_result));
        }
        return Ok(());
    }

    // If we managed to push the message, we return an Ok(()).
    // Otherwise, we return the internal error and return a response failure message.
    if let Err(e) = control_create_transaction_inner(
        m_state,
        ephemeral,
        send_commands,
        max_pending_user_requests,
        create_transaction.clone(),
    ) {
        error!("control_create_transaction_inner() failed: {:?}", e);
        let transaction_result = TransactionResult {
            request_id: create_transaction.request_id,
            result: RequestResult::Failure,
        };

        outgoing_control.push(FunderOutgoingControl::TransactionResult(transaction_result));
    }

    // Every CreateTransaction must have a matching response. Therefore we don't return an error
    // here. We have to make sure the response arrives back to the user.
    Ok(())
}

fn control_request_close_payment<B, R>(
    m_state: &mut MutableFunderState<B>,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    rng: &R,
    payment_id: PaymentId,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    let payment = if let Some(payment) = m_state.state().payments.get(&payment_id) {
        payment
    } else {
        // Payment not found:
        let response_close_payment = ResponseClosePayment {
            payment_id,
            status: PaymentStatus::PaymentNotFound,
        };
        outgoing_control.push(FunderOutgoingControl::ResponseClosePayment(
            response_close_payment,
        ));
        return Ok(());
    };

    let (new_payment_stage, opt_payment_status) = match &payment.stage {
        PaymentStage::NewTransactions(new_transactions) => {
            if new_transactions.num_transactions > 0 {
                (
                    PaymentStage::InProgress(new_transactions.num_transactions),
                    None,
                )
            } else {
                let ack_uid = Uid::rand_gen(rng);
                (
                    PaymentStage::Canceled(ack_uid.clone()),
                    Some(PaymentStatus::Canceled(ack_uid)),
                )
            }
        }
        PaymentStage::InProgress(num_transactions) => {
            assert!(*num_transactions > 0);
            (PaymentStage::InProgress(*num_transactions), None)
        }
        PaymentStage::Success((num_transactions, receipt, ack_uid)) => (
            PaymentStage::Success((*num_transactions, receipt.clone(), ack_uid.clone())),
            Some(PaymentStatus::Success(PaymentStatusSuccess {
                receipt: receipt.clone(),
                ack_uid: ack_uid.clone(),
            })),
        ),
        PaymentStage::Canceled(ack_uid) => (
            PaymentStage::Canceled(ack_uid.clone()),
            Some(PaymentStatus::Canceled(ack_uid.clone())),
        ),
        PaymentStage::AfterSuccessAck(num_transactions) => (
            PaymentStage::AfterSuccessAck(*num_transactions),
            Some(PaymentStatus::PaymentNotFound),
        ),
    };

    // Possibly send back a ResponseClosePayment:
    if let Some(payment_status) = opt_payment_status {
        let response_close_payment = ResponseClosePayment {
            payment_id: payment_id.clone(),
            status: payment_status,
        };
        outgoing_control.push(FunderOutgoingControl::ResponseClosePayment(
            response_close_payment,
        ));
    }

    // We perform no mutations if no changes happened:
    if new_payment_stage == payment.stage {
        return Ok(());
    }

    let new_payment = Payment {
        src_plain_lock: payment.src_plain_lock.clone(),
        stage: new_payment_stage,
    };

    let funder_mutation = FunderMutation::UpdatePayment((payment_id, new_payment));
    m_state.mutate(funder_mutation);
    Ok(())
}

fn control_ack_close_payment<B>(
    m_state: &mut MutableFunderState<B>,
    ack_close_payment: AckClosePayment,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    let payment = m_state
        .state()
        .payments
        .get(&ack_close_payment.payment_id)
        .ok_or(HandleControlError::PaymentDoesNotExist)?
        .clone();

    let Payment { stage, .. } = payment;

    match stage {
        PaymentStage::NewTransactions(_)
        | PaymentStage::InProgress(_)
        | PaymentStage::AfterSuccessAck(_) => return Err(HandleControlError::AckStateInvalid),
        PaymentStage::Success((num_transactions, _receipt, ack_uid)) => {
            // Make sure that ack matches:
            if ack_close_payment.ack_uid != ack_uid {
                return Err(HandleControlError::AckMismatch);
            }

            if num_transactions > 0 {
                // Update payment to be `AfterSuccessAck`:
                let new_payment = Payment {
                    src_plain_lock: payment.src_plain_lock,
                    stage: PaymentStage::AfterSuccessAck(num_transactions),
                };
                let funder_mutation =
                    FunderMutation::UpdatePayment((ack_close_payment.payment_id, new_payment));
                m_state.mutate(funder_mutation);
            } else {
                // Remove payment (no pending transactions):
                let funder_mutation = FunderMutation::RemovePayment(ack_close_payment.payment_id);
                m_state.mutate(funder_mutation);
            }
        }
        PaymentStage::Canceled(ack_uid) => {
            // Make sure that ack matches:
            if ack_close_payment.ack_uid != ack_uid {
                return Err(HandleControlError::AckMismatch);
            }

            // Remove payment:
            let funder_mutation = FunderMutation::RemovePayment(ack_close_payment.payment_id);
            m_state.mutate(funder_mutation);
        }
    };

    Ok(())
}

fn control_add_invoice<B, R>(
    m_state: &mut MutableFunderState<B>,
    rng: &R,
    add_invoice: AddInvoice,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    if m_state
        .state()
        .open_invoices
        .contains_key(&add_invoice.invoice_id)
    {
        return Err(HandleControlError::InvoiceAlreadyExists);
    }

    // Randomly generate a lock. We only reveal this lock when sending the Collect message.
    let dest_plain_lock = PlainLock::rand_gen(rng);

    // Add new invoice:
    let funder_mutation = FunderMutation::AddInvoice((
        add_invoice.invoice_id,
        add_invoice.currency,
        add_invoice.total_dest_payment,
        dest_plain_lock,
    ));
    m_state.mutate(funder_mutation);

    Ok(())
}

fn control_cancel_invoice<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    invoice_id: InvoiceId,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    let open_invoice = m_state
        .state()
        .open_invoices
        .get(&invoice_id)
        .ok_or(HandleControlError::InvoiceDoesNotExist)?
        .clone();

    // Cancel all pending transactions related to this invoice
    for request_id in &open_invoice.incoming_transactions {
        // Explaining the unwrap() below:
        // We expect that the origin of this request must be from an existing friend.
        // We can not be the originator of this request.
        let friend_public_key =
            find_request_origin(m_state.state(), &open_invoice.currency, &request_id)
                .unwrap()
                .clone();
        reply_with_cancel(
            m_state,
            send_commands,
            &friend_public_key,
            &open_invoice.currency,
            &request_id,
        );
    }

    // Remove invoice:
    let funder_mutation = FunderMutation::RemoveInvoice(invoice_id);
    m_state.mutate(funder_mutation);

    Ok(())
}

fn control_commit_invoice<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    commit: &Commit,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Find matching open invoice:
    let open_invoice = m_state
        .state()
        .open_invoices
        .get(&commit.invoice_id)
        .ok_or(HandleControlError::InvoiceDoesNotExist)?
        .clone();

    if !verify_commit(commit, &m_state.state().local_public_key) {
        return Err(HandleControlError::InvalidCommit);
    }

    // The src_plain_lock must match:
    if Some(commit.src_plain_lock.hash_lock()) != open_invoice.opt_src_hashed_lock {
        return Err(HandleControlError::InvalidCommit);
    }

    // Push collect messages for all pending requests
    for request_id in &open_invoice.incoming_transactions {
        let friend_public_key = if let Some(friend_public_key) =
            find_request_origin(m_state.state(), &open_invoice.currency, request_id)
        {
            friend_public_key.clone()
        } else {
            warn!("control_commit_invoice(): Failed to find request origin");
            continue;
        };

        let collect_send_funds = CollectSendFundsOp {
            request_id: request_id.clone(),
            src_plain_lock: commit.src_plain_lock.clone(),
            dest_plain_lock: open_invoice.dest_plain_lock.clone(),
        };

        let friend_mutation = FriendMutation::PushBackPendingBackwardsOp((
            open_invoice.currency.clone(),
            BackwardsOp::Collect(collect_send_funds),
        ));
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);

        // Signal the sender to attempt to send:
        send_commands.set_try_send(&friend_public_key);
    }

    // Remove invoice:
    let funder_mutation = FunderMutation::RemoveInvoice(commit.invoice_id.clone());
    m_state.mutate(funder_mutation);

    Ok(())
}

pub fn handle_control_message<B, R>(
    m_state: &mut MutableFunderState<B>,
    m_ephemeral: &mut MutableEphemeral,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    rng: &R,
    max_node_relays: usize,
    max_pending_user_requests: usize,
    incoming_control: FunderControl<B>,
) -> Result<(), HandleControlError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    match incoming_control {
        FunderControl::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt) => {
            control_set_friend_currency_max_debt(
                m_state,
                send_commands,
                set_friend_currency_max_debt,
            )
        }

        FunderControl::ResetFriendChannel(reset_friend_channel) => {
            control_reset_friend_channel(m_state, send_commands, reset_friend_channel)
        }

        FunderControl::AddRelay(named_relay_address) => control_add_relay(
            m_state,
            send_commands,
            outgoing_channeler_config,
            max_node_relays,
            named_relay_address,
        ),

        FunderControl::RemoveRelay(public_key) => {
            control_remove_relay(
                m_state,
                send_commands,
                outgoing_channeler_config,
                public_key,
            );
            Ok(())
        }

        FunderControl::AddFriend(add_friend) => {
            control_add_friend(m_state, add_friend);
            Ok(())
        }

        FunderControl::RemoveFriend(remove_friend) => control_remove_friend(
            m_state,
            send_commands,
            outgoing_control,
            outgoing_channeler_config,
            rng,
            remove_friend,
        ),

        FunderControl::SetFriendStatus(set_friend_status) => control_set_friend_status(
            m_state,
            send_commands,
            outgoing_control,
            outgoing_channeler_config,
            rng,
            set_friend_status,
        ),

        FunderControl::SetFriendCurrencyRequestsStatus(set_friend_currency_requests_status) => {
            control_set_friend_currency_requests_status(
                m_state,
                send_commands,
                set_friend_currency_requests_status,
            )
        }

        FunderControl::SetFriendRelays(set_friend_relays) => {
            control_set_friend_relays(m_state, outgoing_channeler_config, set_friend_relays)
        }

        FunderControl::SetFriendName(set_friend_name) => {
            control_set_friend_name(m_state, set_friend_name)
        }
        FunderControl::SetFriendCurrencyRate(set_friend_currency_rate) => {
            control_set_friend_currency_rate(m_state, send_commands, set_friend_currency_rate)
        }

        FunderControl::RemoveFriendCurrency(remove_friend_currency) => {
            control_remove_friend_currency(m_state, send_commands, remove_friend_currency)
        }

        // Buyer API:
        FunderControl::CreatePayment(create_payment) => {
            control_create_payment(m_state, rng, create_payment)
        }
        FunderControl::CreateTransaction(create_transaction) => control_create_transaction(
            m_state,
            m_ephemeral.ephemeral(),
            outgoing_control,
            send_commands,
            max_pending_user_requests,
            create_transaction,
        ),
        FunderControl::RequestClosePayment(payment_id) => {
            control_request_close_payment(m_state, outgoing_control, rng, payment_id)
        }
        FunderControl::AckClosePayment(ack_close_payment) => {
            control_ack_close_payment(m_state, ack_close_payment)
        }

        // Seller API:
        FunderControl::AddInvoice(add_invoice) => control_add_invoice(m_state, rng, add_invoice),
        FunderControl::CancelInvoice(invoice_id) => {
            control_cancel_invoice(m_state, send_commands, invoice_id)
        }
        FunderControl::CommitInvoice(commit) => {
            control_commit_invoice(m_state, send_commands, &commit)
        }
    }
}
