use std::fmt::Debug;

use common::canonical_serialize::CanonicalSerialize;

use crypto::identity::PublicKey;

use crate::friend::{FriendMutation, ChannelStatus};
use crate::state::{FunderMutation};


use proto::funder::messages::{FriendStatus, UserRequestSendFunds,
    SetFriendRemoteMaxDebt, ResetFriendChannel, SetFriendRelays, SetFriendName, 
    AddFriend, RemoveFriend, SetFriendStatus, SetRequestsStatus,
    ReceiptAck, ResponseReceived, 
    FunderOutgoingControl, ResponseSendFundsResult,
    ChannelerUpdateFriend, FunderControl};
use proto::app_server::messages::{RelayAddress, NamedRelayAddress};

use crate::ephemeral::Ephemeral;
use crate::handler::handler::{MutableFunderState, MutableEphemeral, is_friend_ready};
use crate::handler::sender::SendCommands;
use crate::handler::canceler::{cancel_local_pending_requests, 
    cancel_pending_user_requests, cancel_pending_requests};

use crate::types::ChannelerConfig;

#[derive(Debug)]
pub enum HandleControlError {
    FriendDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
    NotFirstInRoute,
    InvalidRoute,
    RequestAlreadyInProgress,
    PendingUserRequestsFull,
    ReceiptDoesNotExist,
    ReceiptSignatureMismatch,
    UserRequestInvalid,
    FriendNotReady,
    MaxNodeRelaysReached,
}


fn control_set_friend_remote_max_debt<B>(m_state: &mut MutableFunderState<B>,
                                         send_commands: &mut SendCommands,
                                         set_friend_remote_max_debt: SetFriendRemoteMaxDebt) 
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    // Make sure that friend exists:
    let friend = m_state.state().friends.get(&set_friend_remote_max_debt.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    if friend.wanted_remote_max_debt == set_friend_remote_max_debt.remote_max_debt {
        // Wanted remote max debt is already set to this value. Nothing to do here.
        return Ok(())
    }

    // We only set the wanted remote max debt here. The actual remote max debt will be changed
    // only when we manage to send a move token message containing the SetRemoteMaxDebt
    // operation.
    let friend_mutation = FriendMutation::SetWantedRemoteMaxDebt(set_friend_remote_max_debt.remote_max_debt);
    let m_mutation = FunderMutation::FriendMutation(
        (set_friend_remote_max_debt.friend_public_key.clone(), friend_mutation));
    m_state.mutate(m_mutation);

    send_commands.set_try_send(&set_friend_remote_max_debt.friend_public_key);
    Ok(())
}

fn control_reset_friend_channel<B>(m_state: &mut MutableFunderState<B>,
                                send_commands: &mut SendCommands,
                                reset_friend_channel: ResetFriendChannel) 
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    let friend = m_state.state().friends.get(&reset_friend_channel.friend_public_key)
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
                },
            }
        },
    }?;

    // We don't have the ability to sign here, therefore we defer the creation
    // of the local reset outgoing move token to the sender.
    send_commands.set_local_reset(&reset_friend_channel.friend_public_key);

    Ok(())
}

fn enable_friend<B>(m_state: &mut MutableFunderState<B>,
                 outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
                 friend_public_key: &PublicKey,
                 friend_relays: &Vec<RelayAddress<B>>) 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Notify Channeler:
    let channeler_add_friend = ChannelerUpdateFriend {
        friend_public_key: friend_public_key.clone(),
        friend_relays: friend_relays.clone(),
        local_relays: friend.sent_local_relays.to_vec(),
    };
    let channeler_config = ChannelerConfig::UpdateFriend(channeler_add_friend);
    outgoing_channeler_config.push(channeler_config);

}

fn disable_friend<B>(m_state: &mut MutableFunderState<B>,
                     send_commands: &mut SendCommands,
                     outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
                     outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
                     friend_public_key: &PublicKey) 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    // Cancel all pending requests to this friend:
    cancel_pending_requests(m_state,
                            send_commands,
                            outgoing_control,
                            friend_public_key);

    cancel_pending_user_requests(m_state,
                                 outgoing_control,
                                 friend_public_key);

    // Notify Channeler:
    let channeler_config = ChannelerConfig::RemoveFriend(
        friend_public_key.clone());
    outgoing_channeler_config.push(channeler_config);

}

fn control_add_relay<B>(m_state: &mut MutableFunderState<B>, 
                          send_commands: &mut SendCommands,
                          outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
                          max_node_relays: usize,
                          named_relay_address: NamedRelayAddress<B>) -> Result<(), HandleControlError>
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
    let friend_public_keys = m_state.state().friends.keys()
        .cloned()
        .collect::<Vec<_>>();
    
    for friend_public_key in &friend_public_keys {
        send_commands.set_try_send(friend_public_key);
    }
    Ok(())
}

fn control_remove_relay<B>(m_state: &mut MutableFunderState<B>, 
                          send_commands: &mut SendCommands,
                          outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
                          public_key: PublicKey) 
where
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
    let friend_public_keys = m_state.state().friends.keys()
        .cloned()
        .collect::<Vec<_>>();
    
    for friend_public_key in &friend_public_keys {
        send_commands.set_try_send(friend_public_key);
    }
}

fn control_add_friend<B>(m_state: &mut MutableFunderState<B>, 
                         add_friend: AddFriend<B>) 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    let funder_mutation = FunderMutation::AddFriend(add_friend.clone());
    m_state.mutate(funder_mutation); }

/// This is a violent operation, as it removes all the known state with the remote friend.  
/// An inconsistency will occur if the friend is added again.
fn control_remove_friend<B>(m_state: &mut MutableFunderState<B>,
                            send_commands: &mut SendCommands,
                            outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
                            outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
                            remove_friend: RemoveFriend) 
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    // Make sure that friend exists:
    let _friend = m_state.state().friends.get(&remove_friend.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    disable_friend(m_state, 
                   send_commands,
                   outgoing_control,
                   outgoing_channeler_config, 
                   &remove_friend.friend_public_key);

    cancel_local_pending_requests(m_state,
                                  send_commands,
                                  outgoing_control,
                                  &remove_friend.friend_public_key);

    let funder_mutation = FunderMutation::RemoveFriend(
            remove_friend.friend_public_key.clone());
    m_state.mutate(funder_mutation);

    Ok(())
}

fn control_set_friend_status<B>(m_state: &mut MutableFunderState<B>,
                                send_commands: &mut SendCommands,
                                outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
                                outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
                                set_friend_status: SetFriendStatus) 
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    // Make sure that friend exists:
    let _ = m_state.state().friends.get(&set_friend_status.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    let friend_mutation = FriendMutation::SetStatus(set_friend_status.status.clone());
    let funder_mutation = FunderMutation::FriendMutation(
        (set_friend_status.friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    let friend = m_state.state().friends.get(&set_friend_status.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    let friend_public_key = &set_friend_status.friend_public_key;
    let friend_address = friend.remote_relays.clone();

    match set_friend_status.status {
        FriendStatus::Enabled => enable_friend(m_state, 
                                               outgoing_channeler_config,
                                               friend_public_key, 
                                               &friend_address),
        FriendStatus::Disabled => disable_friend(m_state, 
                                                 send_commands,
                                                 outgoing_control,
                                                 outgoing_channeler_config, 
                                                 &friend_public_key),
    };

    Ok(())
}

fn control_set_requests_status<B>(m_state: &mut MutableFunderState<B>,
                                  send_commands: &mut SendCommands,
                               set_requests_status: SetRequestsStatus) 
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    // Make sure that friend exists:
    let _friend = m_state.state().friends.get(&set_requests_status.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    let friend_mutation = FriendMutation::SetWantedLocalRequestsStatus(set_requests_status.status);
    let funder_mutation = FunderMutation::FriendMutation(
        (set_requests_status.friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    send_commands.set_try_send(&set_requests_status.friend_public_key);
    Ok(())
}

fn control_set_friend_relays<B>(m_state: &mut MutableFunderState<B>, 
                                 outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
                                 set_friend_relays: SetFriendRelays<B>)
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    // Make sure that friend exists:
    let friend = m_state.state().friends.get(&set_friend_relays.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;


    // If the newly proposed address is the same as the old one,
    // we do nothing:
    if set_friend_relays.relays == friend.remote_relays {
        return Ok(())
    }

    let local_relays = friend.sent_local_relays.to_vec();

    let friend_mutation = FriendMutation::SetRemoteRelays(set_friend_relays.relays.clone());
    let funder_mutation = FunderMutation::FriendMutation(
        (set_friend_relays.friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    // Notify Channeler to change the friend's address:
    let update_friend = ChannelerUpdateFriend {
        friend_public_key: set_friend_relays.friend_public_key.clone(),
        friend_relays: set_friend_relays.relays.clone(),
        local_relays,
    };
    let channeler_config = ChannelerConfig::UpdateFriend(update_friend);
    outgoing_channeler_config.push(channeler_config);

    Ok(())
}

fn control_set_friend_name<B>(m_state: &mut MutableFunderState<B>,
                              set_friend_name: SetFriendName)
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    // Make sure that friend exists:
    let friend = m_state.state().friends.get(&set_friend_name.friend_public_key)
        .ok_or(HandleControlError::FriendDoesNotExist)?;

    // If the newly proposed name is the same as the old one, we do nothing:
    if friend.name == set_friend_name.name {
        return Ok(())
    }

    let friend_mutation = FriendMutation::SetName(set_friend_name.name);
    let funder_mutation = FunderMutation::FriendMutation(
        (set_friend_name.friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    Ok(())
}

fn check_user_request_valid(user_request_send_funds: &UserRequestSendFunds) 
    -> Option<()> {

    if !user_request_send_funds.route.is_valid() {
        return None;
    }
    Some(())
}

fn control_request_send_funds_inner<B>(m_state: &mut MutableFunderState<B>,
                                       ephemeral: &Ephemeral,
                                       outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
                                       send_commands: &mut SendCommands,
                                       max_pending_user_requests: usize,
                                       user_request_send_funds: UserRequestSendFunds)
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    check_user_request_valid(&user_request_send_funds)
        .ok_or(HandleControlError::UserRequestInvalid)?;

    // If we already have a receipt for this request, we return the receipt immediately and
    // exit. Note that we don't erase the receipt yet. This will only be done when a receipt
    // ack is received.
    if let Some(receipt) = m_state.state().ready_receipts.get(&user_request_send_funds.request_id) {
        let response_received = ResponseReceived {
            request_id: user_request_send_funds.request_id,
            result: ResponseSendFundsResult::Success(receipt.clone()),
        };
        outgoing_control.push(FunderOutgoingControl::ResponseReceived(response_received));
        return Ok(());
    }

    let route = &user_request_send_funds.route;

    // We have to be the first on the route:
    match route.public_keys.first() {
        Some(first) if *first == m_state.state().local_public_key => Ok(()),
        _ => Err(HandleControlError::NotFirstInRoute),
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

    if !is_friend_ready(m_state.state(), 
                        ephemeral, 
                        &friend_public_key) {
        return Err(HandleControlError::FriendNotReady);
    }

    // If request is already in progress, we do nothing:
    // Check if there is already a pending user request with the same request_id:
    for user_request in &friend.pending_user_requests {
        if user_request_send_funds.request_id == user_request.request_id {
            return Err(HandleControlError::RequestAlreadyInProgress);
        }
    }

    let token_channel = match &friend.channel_status {
        ChannelStatus::Inconsistent(_) => unreachable!(),
        ChannelStatus::Consistent(token_channel) => token_channel
    };

    // Check if there is an onging request with the same request_id with this specific friend:
    if token_channel
        .get_mutual_credit()
        .state()
        .pending_requests
        .pending_local_requests
        .contains_key(&user_request_send_funds.request_id) {
            return Err(HandleControlError::RequestAlreadyInProgress);
    }

    // Check if we have room to push this message:
    if friend.pending_user_requests.len() >= max_pending_user_requests {
        return Err(HandleControlError::PendingUserRequestsFull);
    }

    let request_send_funds = user_request_send_funds.to_request();
    let friend_mutation = FriendMutation::PushBackPendingUserRequest(request_send_funds);
    let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);
    send_commands.set_try_send(&friend_public_key);

    Ok(())
}


fn control_request_send_funds<B>(m_state: &mut MutableFunderState<B>, 
                                 ephemeral: &Ephemeral,
                                 outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
                                 send_commands: &mut SendCommands,
                                 max_pending_user_requests: usize,
                                 user_request_send_funds: UserRequestSendFunds) 
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    
    // If we managed to push the message, we return an Ok(()).
    // Otherwise, we return the internal error and return a response failure message.
    if let Err(e) = control_request_send_funds_inner(m_state, 
                                     ephemeral,
                                     outgoing_control, 
                                     send_commands, 
                                     max_pending_user_requests,
                                     user_request_send_funds.clone()) {

        error!("control_request_send_funds_inner() failed: {:?}", e);
        let response_received = ResponseReceived {
            request_id: user_request_send_funds.request_id,
            result: ResponseSendFundsResult::Failure(m_state.state().local_public_key.clone()),
        };

        outgoing_control.push(FunderOutgoingControl::ResponseReceived(response_received));
    }

    // Every RequestSendFunds must have a matching response. Therefore we don't return an error
    // here. We have to make sure the response arrives back to the user.
    Ok(())
}

/// Handle an incoming receipt ack message
fn control_receipt_ack<B>(m_state: &mut MutableFunderState<B>, 
                          receipt_ack: ReceiptAck) 
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    let receipt = m_state.state().ready_receipts.get(&receipt_ack.request_id)
        .ok_or(HandleControlError::ReceiptDoesNotExist)?;

    // Make sure that the provided signature matches the one we have at the ready receipt.
    // We do this to make sure the user doesn't send a receipt ack before he actually got the
    // receipt (The user can not predit the receipt_signature ahead of time)
    if receipt_ack.receipt_signature != receipt.signature {
        return Err(HandleControlError::ReceiptSignatureMismatch);
    }

    let funder_mutation = FunderMutation::RemoveReceipt(receipt_ack.request_id);
    m_state.mutate(funder_mutation);

    Ok(())
}


pub fn handle_control_message<B>(m_state: &mut MutableFunderState<B>,
                                 m_ephemeral: &mut MutableEphemeral,
                                 send_commands: &mut SendCommands,
                                 outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
                                 outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
                                 max_node_relays: usize,
                                 max_pending_user_requests: usize,
                                 incoming_control: FunderControl<B>) 
    -> Result<(), HandleControlError> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{

    match incoming_control {
        FunderControl::SetFriendRemoteMaxDebt(set_friend_remote_max_debt) =>
            control_set_friend_remote_max_debt(m_state, 
                                               send_commands, 
                                               set_friend_remote_max_debt),

        FunderControl::ResetFriendChannel(reset_friend_channel) =>
            control_reset_friend_channel(m_state, 
                                         send_commands,
                                         reset_friend_channel),

        FunderControl::AddRelay(named_relay_address) =>
            control_add_relay(m_state, 
                                send_commands,
                                outgoing_channeler_config,
                                max_node_relays,
                                named_relay_address),

        FunderControl::RemoveRelay(public_key) =>
            Ok(control_remove_relay(m_state, 
                                    send_commands,
                                    outgoing_channeler_config,
                                    public_key)),

        FunderControl::AddFriend(add_friend) =>
            Ok(control_add_friend(m_state, 
                               add_friend)),

        FunderControl::RemoveFriend(remove_friend) =>
            control_remove_friend(m_state,
                                  send_commands,
                                  outgoing_control,
                                  outgoing_channeler_config,
                                  remove_friend),

        FunderControl::SetFriendStatus(set_friend_status) =>
            control_set_friend_status(m_state, 
                                      send_commands,
                                      outgoing_control,
                                      outgoing_channeler_config,
                                      set_friend_status),

        FunderControl::SetRequestsStatus(set_requests_status) =>
            control_set_requests_status(m_state, 
                                        send_commands,
                                        set_requests_status),

        FunderControl::SetFriendRelays(set_friend_relays) =>
            control_set_friend_relays(m_state, 
                                       outgoing_channeler_config,
                                       set_friend_relays),

        FunderControl::SetFriendName(set_friend_name) =>
            control_set_friend_name(m_state, 
                                    set_friend_name),

        FunderControl::RequestSendFunds(user_request_send_funds) =>
            control_request_send_funds(m_state, 
                                       m_ephemeral.ephemeral(),
                                       outgoing_control, 
                                       send_commands,
                                       max_pending_user_requests,
                                       user_request_send_funds),

        FunderControl::ReceiptAck(receipt_ack) =>
            control_receipt_ack(m_state, 
                                receipt_ack),
    }
}
