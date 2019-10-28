use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use im::hashset::HashSet as ImHashSet;

use signature::canonical::CanonicalSerialize;

use crypto::rand::{CryptoRandom, RandGen};

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::{PublicKey, RandValue};
use proto::funder::messages::{
    BalanceInfo, ChannelerUpdateFriend, CountersInfo, Currency, CurrencyBalanceInfo,
    CurrencyOperations, FriendMessage, FriendTcOp, FunderOutgoingControl, McInfo, MoveTokenRequest,
    RequestResult, RequestsStatus, TokenInfo, TransactionResult,
};

use identity::IdentityClient;

use crate::mutual_credit::outgoing::{OutgoingMc, QueueOperationError};
use crate::types::{
    create_cancel_send_funds, create_pending_transaction, create_unsigned_move_token,
    sign_move_token, ChannelerConfig,
};

use crate::friend::{
    BackwardsOp, ChannelInconsistent, ChannelStatus, CurrencyConfig, FriendMutation,
    SentLocalRelays,
};
use crate::token_channel::{SendMoveTokenOutput, SetDirection, TcMutation, TokenChannel};

use crate::ephemeral::Ephemeral;
use crate::handler::canceler::remove_transaction;
use crate::handler::state_wrap::MutableFunderState;
use crate::handler::types::{FriendSendCommands, SendCommands};
use crate::handler::utils::find_request_origin;
use crate::state::{FunderMutation, FunderState};

pub type OutgoingMessage<B> = (PublicKey, FriendMessage<B>);

#[derive(Debug)]
enum PendingQueueError {
    InsufficientTrust,
    MaxOperationsReached,
}

#[derive(Debug)]
enum CollectOutgoingError {
    MaxOperationsReached,
}

#[derive(Debug)]
/// Pending messages we want to send through a single currency
/// in the next MoveToken.
struct PendingCurrency {
    /// Mutual credit state for this currency, allowing us to simulate sending operations
    /// through this channel.
    outgoing_mc: OutgoingMc,
    /// Pending outgoing operations
    operations: Vec<FriendTcOp>,
}

struct PendingMoveToken<B> {
    friend_public_key: PublicKey,
    pending_currencies: HashMap<Currency, PendingCurrency>,
    opt_local_relays: Option<Vec<RelayAddress<B>>>,
    opt_active_currencies: Option<Vec<Currency>>,
    token_wanted: bool,
    max_operations_in_batch: usize,
    /// Can we send this move token with empty operations list
    /// and empty opt_local_address?
    may_send_empty: bool,
}

impl<B> PendingMoveToken<B>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    fn new(
        friend_public_key: PublicKey,
        max_operations_in_batch: usize,
        may_send_empty: bool,
    ) -> Self {
        PendingMoveToken {
            friend_public_key,
            pending_currencies: HashMap::new(),
            opt_local_relays: None,
            opt_active_currencies: None,
            token_wanted: false,
            max_operations_in_batch,
            may_send_empty,
        }
    }

    /// Attempt to queue one operation into a certain `pending_move_token`.
    /// If successful, mutations are applied and the operation is queued.
    /// Otherwise, an error is returned.
    fn queue_operation(
        &mut self,
        currency: &Currency,
        operation: &FriendTcOp,
        m_state: &mut MutableFunderState<B>,
    ) -> Result<(), PendingQueueError> {
        // Make sure we do not have too many operations queued:
        let num_operations: usize = self
            .pending_currencies
            .values()
            .map(|pending_currency| pending_currency.operations.len())
            .sum();
        if num_operations >= self.max_operations_in_batch {
            return Err(PendingQueueError::MaxOperationsReached);
        }

        let friend = m_state
            .state()
            .friends
            .get(&self.friend_public_key)
            .unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };

        let outgoing_mc = token_channel
            .get_incoming()
            .unwrap()
            .create_outgoing_mc(currency)
            .unwrap();
        let pending_currency =
            self.pending_currencies
                .entry(currency.clone())
                .or_insert(PendingCurrency {
                    outgoing_mc,
                    operations: Vec::new(),
                });

        let _mc_mutations = match pending_currency.outgoing_mc.queue_operation(operation) {
            Ok(mc_mutations) => Ok(mc_mutations),
            Err(QueueOperationError::RequestAlreadyExists) => {
                warn!("Request already exists: {:?}", operation);
                Ok(vec![])
            }
            Err(QueueOperationError::InsufficientTrust) => {
                Err(PendingQueueError::InsufficientTrust)
            }
            Err(_) => unreachable!(),
        }?;

        // Add operation:
        pending_currency.operations.push(operation.clone());

        /*
        // Apply mutations:
        for mc_mutation in mc_mutations {
            let tc_mutation = TcMutation::McMutation((currency.clone(), mc_mutation));
            let friend_mutation = FriendMutation::TcMutation(tc_mutation);
            let funder_mutation =
                FunderMutation::FriendMutation((self.friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);
        }
        */

        Ok(())
    }

    fn set_local_relays(&mut self, local_relays: Vec<RelayAddress<B>>) {
        self.opt_local_relays = Some(local_relays);
    }

    fn set_active_currencies(&mut self, active_currencies: Vec<Currency>) {
        self.opt_active_currencies = Some(active_currencies);
    }
}

fn transmit_outgoing<B>(
    m_state: &MutableFunderState<B>,
    friend_public_key: &PublicKey,
    token_wanted: bool,
    outgoing_messages: &mut Vec<OutgoingMessage<B>>,
) where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    let move_token = token_channel
        .get_outgoing()
        .unwrap()
        .create_outgoing_move_token();

    let move_token_request = MoveTokenRequest {
        move_token,
        token_wanted,
    };

    outgoing_messages.push((
        friend_public_key.clone(),
        FriendMessage::MoveTokenRequest(move_token_request),
    ));
}

pub async fn apply_local_reset<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    friend_public_key: &'a PublicKey,
    channel_inconsistent: &'a ChannelInconsistent,
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
) where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug + Hash,
    R: CryptoRandom,
{
    // TODO: How to do this without unwrap?:
    let remote_reset_terms = channel_inconsistent.opt_remote_reset_terms.clone().unwrap();

    // Update last_sent_relays:
    let sent_local_relays = &m_state
        .state()
        .friends
        .get(friend_public_key)
        .unwrap()
        .sent_local_relays;
    let new_sent_local_relays = match sent_local_relays {
        SentLocalRelays::NeverSent => SentLocalRelays::LastSent(m_state.state().relays.clone()),
        SentLocalRelays::Transition((last_sent, before_last_sent)) => {
            // We fear that the remote side might lose our relays (After the reset)
            // To be on the safe side, we take all the relays the remote side might know about us,
            // and set them as the old relays.
            let last_sent_set: HashSet<NamedRelayAddress<B>> =
                last_sent.clone().into_iter().collect();
            let before_last_sent_set: HashSet<NamedRelayAddress<B>> =
                before_last_sent.clone().into_iter().collect();
            let old_local_relays = last_sent_set
                .union(&before_last_sent_set)
                .cloned()
                .collect();
            SentLocalRelays::Transition((m_state.state().relays.clone(), old_local_relays))
        }
        SentLocalRelays::LastSent(last_sent) => {
            SentLocalRelays::Transition((m_state.state().relays.clone(), last_sent.clone()))
        }
    };

    let friend_mutation = FriendMutation::SetSentLocalRelays(new_sent_local_relays);
    let funder_mutation =
        FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    // Prepare our current relays:
    let opt_local_relays = Some(
        m_state
            .state()
            .relays
            .clone()
            .into_iter()
            .map(RelayAddress::from)
            .collect(),
    );

    let move_token_counter = 0;
    let local_pending_debt = 0;
    let remote_pending_debt = 0;
    let opt_active_currencies = None;

    let balances = remote_reset_terms
        .balance_for_reset
        .iter()
        .map(|currency_balance| CurrencyBalanceInfo {
            currency: currency_balance.currency.clone(),
            balance_info: BalanceInfo {
                balance: currency_balance.balance.checked_neg().unwrap(),
                local_pending_debt,
                remote_pending_debt,
            },
        })
        .collect();

    let token_info = TokenInfo {
        mc: McInfo {
            local_public_key: m_state.state().local_public_key.clone(),
            remote_public_key: friend_public_key.clone(),
            balances,
        },
        counters: CountersInfo {
            inconsistency_counter: remote_reset_terms.inconsistency_counter,
            move_token_counter,
        },
    };

    let rand_nonce = RandValue::rand_gen(rng);
    let u_reset_move_token = create_unsigned_move_token(
        // No operations are required for a reset move token
        Vec::new(),
        opt_local_relays,
        opt_active_currencies,
        &token_info,
        remote_reset_terms.reset_token.clone(),
        rand_nonce,
    );

    let reset_move_token = sign_move_token(u_reset_move_token, identity_client).await;

    let token_channel = TokenChannel::new_from_local_reset(
        &reset_move_token,
        &token_info,
        channel_inconsistent.opt_last_incoming_move_token.clone(),
    );

    let friend_mutation = FriendMutation::SetConsistent(token_channel);
    let funder_mutation =
        FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    // Add possibly missing currency configurations:
    let currency_configs = m_state
        .state()
        .friends
        .get(friend_public_key)
        .unwrap()
        .currency_configs
        .clone();
    for currency_balance in &remote_reset_terms.balance_for_reset {
        if !currency_configs.contains_key(&currency_balance.currency) {
            // This is a reset message. We reset the token channel:
            let friend_mutation = FriendMutation::UpdateCurrencyConfig((
                currency_balance.currency.clone(),
                CurrencyConfig::new(),
            ));
            let funder_mutation =
                FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);
        }
    }
}

async fn send_friend_iter1<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    friend_public_key: &'a PublicKey,
    friend_send_commands: &'a FriendSendCommands,
    pending_move_tokens: &'a mut HashMap<PublicKey, PendingMoveToken<B>>,
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
    max_operations_in_batch: usize,
    cancel_public_keys: &'a mut HashSet<PublicKey>,
    mut outgoing_messages: &'a mut Vec<OutgoingMessage<B>>,
    outgoing_control: &'a mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &'a mut Vec<ChannelerConfig<RelayAddress<B>>>,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug + Hash,
    R: CryptoRandom,
{
    // If we got here, it must be because some send_commands attribute was set:
    assert!(
        friend_send_commands.try_send
            || friend_send_commands.resend_relays
            || friend_send_commands.resend_outgoing
            || friend_send_commands.remote_wants_token
            || friend_send_commands.local_reset
    );

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Check if we need to perform a local reset:
    if friend_send_commands.local_reset {
        if let ChannelStatus::Inconsistent(channel_inconsistent) = &friend.channel_status {
            let c_channel_inconsistent = channel_inconsistent.clone();
            apply_local_reset(
                m_state,
                friend_public_key,
                &c_channel_inconsistent,
                identity_client,
                rng,
            )
            .await;

            // TODO: Is the token wanted after reset?
            let is_token_wanted = true;
            transmit_outgoing(
                m_state,
                friend_public_key,
                is_token_wanted,
                &mut outgoing_messages,
            );
            return;
        } else {
            unreachable!();
        }
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
        ChannelStatus::Inconsistent(channel_inconsistent) => {
            if friend_send_commands.resend_outgoing || friend_send_commands.try_send {
                outgoing_messages.push((
                    friend_public_key.clone(),
                    FriendMessage::InconsistencyError(
                        channel_inconsistent.local_reset_terms.clone(),
                    ),
                ));
            }
            return;
        }
    };

    let token_channel = &channel_consistent.token_channel;

    if let Some(tc_out_borrow) = &token_channel.get_outgoing() {
        // Remote side has the token:

        let tc_outgoing = &tc_out_borrow.tc_outgoing;
        // Do we have anything that we want to send?
        // (Currently the token is at the remote side)
        let is_pending = estimate_should_send(m_state.state(), friend_public_key);
        if is_pending || friend_send_commands.resend_outgoing {
            let is_token_wanted =
                is_pending || tc_outgoing.move_token_out.opt_local_relays.is_some();
            transmit_outgoing(
                m_state,
                &friend_public_key,
                is_token_wanted,
                &mut outgoing_messages,
            );
        }

        return;
    }

    // If we are here, the token channel is incoming:

    // TODO: Make sure resend_outgoing is set smartly in handle_liveness
    // (Only in case of inconsistency?, or outgoing direction)
    //
    // It will be strange if we need to resend outgoing, because the channel
    // is in incoming mode.
    // -- This could happen in handle_liveness.
    // assert!(!friend_send_commands.resend_outgoing);

    let may_send_empty =
        friend_send_commands.resend_outgoing || friend_send_commands.remote_wants_token;
    let pending_move_token = PendingMoveToken::new(
        friend_public_key.clone(),
        max_operations_in_batch,
        may_send_empty,
    );

    // TODO: Find a more elegant way to insert and get_mut() right after it?
    pending_move_tokens.insert(friend_public_key.clone(), pending_move_token);
    let pending_move_token = pending_move_tokens.get_mut(friend_public_key).unwrap();

    let _ = collect_outgoing_move_token(
        m_state,
        rng,
        outgoing_channeler_config,
        outgoing_control,
        cancel_public_keys,
        friend_public_key,
        pending_move_token,
        friend_send_commands.resend_relays,
    );
}

/// Do we need to send anything to the remote side?
/// Note that this is only an estimation. It is possible that when the token from remote side
/// arrives, the state will be different.
fn estimate_should_send<'a, B>(state: &'a FunderState<B>, friend_public_key: &'a PublicKey) -> bool
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Check if notification about local address change is required:
    let friend = state.friends.get(friend_public_key).unwrap();
    match &friend.sent_local_relays {
        SentLocalRelays::NeverSent => return true,
        SentLocalRelays::Transition((relays, _)) | SentLocalRelays::LastSent(relays) => {
            if relays != &state.relays {
                return true;
            }
        }
    };

    // Check if update to remote_max_debt is required:
    match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => {
            // Check if we need to tell remote side about our local active currencies:
            let wanted_local_currencies: ImHashSet<_> =
                friend.currency_configs.keys().cloned().collect();
            if wanted_local_currencies
                != channel_consistent
                    .token_channel
                    .get_active_currencies()
                    .local
            {
                return true;
            }

            for (currency, currency_config) in &friend.currency_configs {
                if let Some(mutual_credit) = channel_consistent
                    .token_channel
                    .get_mutual_credits()
                    .get(currency)
                {
                    // We need to change remote_max_debt:
                    if currency_config.wanted_remote_max_debt
                        != mutual_credit.state().balance.remote_max_debt
                    {
                        return true;
                    }

                    // We need to change local requests status:
                    if currency_config.wanted_local_requests_status
                        != mutual_credit.state().requests_status.local
                    {
                        return true;
                    }
                }
            }

            if !channel_consistent.pending_backwards_ops.is_empty()
                || !channel_consistent.pending_requests.is_empty()
                || !channel_consistent.pending_user_requests.is_empty()
            {
                return true;
            }
        }
        ChannelStatus::Inconsistent(_) => {}
    };

    false
}

/// Queue an operation to a PendingMoveToken.
/// On cancel, queue a Cancel message to the relevant friend,
/// or (if we are the origin of the request): send a failure notification through the control.
fn queue_operation_or_cancel<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    rng: &'a R,
    pending_move_token: &'a mut PendingMoveToken<B>,
    cancel_public_keys: &'a mut HashSet<PublicKey>,
    outgoing_control: &'a mut Vec<FunderOutgoingControl<B>>,
    currency: &Currency,
    operation: &'a FriendTcOp,
) -> Result<(), CollectOutgoingError>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
    R: CryptoRandom,
{
    match pending_move_token.queue_operation(currency, operation, m_state) {
        Ok(()) => return Ok(()),
        Err(PendingQueueError::MaxOperationsReached) => {
            pending_move_token.token_wanted = true;
            // We will send this message next time we have the token:
            return Err(CollectOutgoingError::MaxOperationsReached);
        }
        Err(PendingQueueError::InsufficientTrust) => {}
    };

    // The operation must have been a request if we had one of the above errors:
    let request_send_funds = match operation {
        FriendTcOp::RequestSendFunds(request_send_funds) => request_send_funds,
        _ => unreachable!(),
    };

    // We are here if an error occurred.
    // We cancel the request:
    match find_request_origin(m_state.state(), currency, &request_send_funds.request_id).cloned() {
        Some(origin_public_key) => {
            // The friend with public key `origin_public_key` is the origin of this request.
            // We send him back a Cancel message:

            let pending_transaction = create_pending_transaction(request_send_funds);
            let cancel_send_funds =
                BackwardsOp::Cancel(create_cancel_send_funds(pending_transaction.request_id));
            let friend_mutation =
                FriendMutation::PushBackPendingBackwardsOp((currency.clone(), cancel_send_funds));
            let funder_mutation =
                FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);

            cancel_public_keys.insert(origin_public_key.clone());
        }
        None => {
            // We are the origin of this request
            remove_transaction(m_state, outgoing_control, rng, &request_send_funds.request_id);

            let transaction_result = TransactionResult {
                request_id: request_send_funds.request_id.clone(),
                result: RequestResult::Failure,
            };
            outgoing_control.push(FunderOutgoingControl::TransactionResult(transaction_result));
        }
    }

    Ok(())
}

fn backwards_op_to_friend_tc_op(backwards_op: BackwardsOp) -> FriendTcOp {
    match backwards_op {
        BackwardsOp::Response(response_send_funds) => {
            FriendTcOp::ResponseSendFunds(response_send_funds)
        }
        BackwardsOp::Cancel(cancel_send_funds) => FriendTcOp::CancelSendFunds(cancel_send_funds),
        BackwardsOp::Collect(collect_send_funds) => {
            FriendTcOp::CollectSendFunds(collect_send_funds)
        }
    }
}

/// Given a friend with an incoming move token state, create the largest possible move token to
/// send to the remote side.
/// Requests that fail to be processed are moved to the cancel queues of the relevant friends.
fn collect_outgoing_move_token<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    rng: &'a R,
    outgoing_channeler_config: &'a mut Vec<ChannelerConfig<RelayAddress<B>>>,
    outgoing_control: &'a mut Vec<FunderOutgoingControl<B>>,
    cancel_public_keys: &'a mut HashSet<PublicKey>,
    friend_public_key: &'a PublicKey,
    pending_move_token: &'a mut PendingMoveToken<B>,
    resend_relays: bool,
) -> Result<(), CollectOutgoingError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug + Hash,
    R: CryptoRandom,
{
    /*
    - Check if last sent local address is up to date.
    - Collect as many operations as possible (Not more than max ops per batch)
        1. Responses (response, cancel, collect)
        2. Pending requests
        3. User pending requests
    - When adding requests, check the following:
        - Valid from credits point of view.
    - If a request is not valid, queue a Cancel message to relevant friend.
    */

    // Send update about local address if needed:
    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let local_named_relays = m_state.state().relays.clone();

    let local_relays: Vec<_> = local_named_relays
        .iter()
        .cloned()
        .map(RelayAddress::from)
        .collect();

    let opt_new_sent_local_relays = if resend_relays {
        // We need to resend relays, due to a reset (that was initiated from remote side):
        Some(match &friend.sent_local_relays {
            SentLocalRelays::NeverSent => SentLocalRelays::LastSent(m_state.state().relays.clone()),
            SentLocalRelays::Transition((last_sent, before_last_sent)) => {
                // We fear that the remote side might lose our relays (After the reset)
                // To be on the safe side, we take all the relays the remote side might know about us,
                // and set them as the old relays.
                let last_sent_set: HashSet<NamedRelayAddress<B>> =
                    last_sent.clone().into_iter().collect();
                let before_last_sent_set: HashSet<NamedRelayAddress<B>> =
                    before_last_sent.clone().into_iter().collect();
                let old_local_relays = last_sent_set
                    .union(&before_last_sent_set)
                    .cloned()
                    .collect();
                SentLocalRelays::Transition((m_state.state().relays.clone(), old_local_relays))
            }
            SentLocalRelays::LastSent(last_sent) => {
                SentLocalRelays::Transition((m_state.state().relays.clone(), last_sent.clone()))
            }
        })
    } else {
        match &friend.sent_local_relays {
            SentLocalRelays::NeverSent => {
                Some(SentLocalRelays::LastSent(local_named_relays.clone()))
            }
            SentLocalRelays::Transition((last_sent_local_relays, _))
            | SentLocalRelays::LastSent(last_sent_local_relays) => {
                if &local_named_relays != last_sent_local_relays {
                    Some(SentLocalRelays::Transition((
                        local_named_relays.clone(),
                        last_sent_local_relays.clone(),
                    )))
                } else {
                    None
                }
            }
        }
    };

    // Update friend.sent_local_relays accordingly:
    if let Some(new_sent_local_relays) = opt_new_sent_local_relays {
        pending_move_token.set_local_relays(local_relays.clone());
        let friend_mutation = FriendMutation::SetSentLocalRelays(new_sent_local_relays);
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);

        let friend = m_state.state().friends.get(friend_public_key).unwrap();

        // Notify Channeler to change the friend's address:
        let update_friend = ChannelerUpdateFriend {
            friend_public_key: friend_public_key.clone(),
            friend_relays: friend.remote_relays.clone(),
            local_relays: friend.sent_local_relays.to_vec(),
        };
        let channeler_config = ChannelerConfig::UpdateFriend(update_friend);
        outgoing_channeler_config.push(channeler_config);
    }

    // Deal with active currencies:
    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    let wanted_local_currencies: ImHashSet<_> = friend.currency_configs.keys().cloned().collect();
    if wanted_local_currencies != token_channel.get_active_currencies().local {
        // Assert that all active currencies must be included inside the wanted_active_currencies:
        assert!(token_channel
            .get_active_currencies()
            .calc_active()
            .is_subset(&wanted_local_currencies));
        pending_move_token.set_active_currencies(wanted_local_currencies.into_iter().collect());
    }

    // Set remote_max_debt (if required):
    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    for (currency, currency_config) in friend.currency_configs.clone() {
        // Set remote_max_debt and local_requests_status (if required):
        let friend = m_state.state().friends.get(friend_public_key).unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        if let Some(mutual_credit) = token_channel.get_mutual_credits().get(&currency) {
            // We need to change remote_max_debt:
            if currency_config.wanted_remote_max_debt
                != mutual_credit.state().balance.remote_max_debt
            {
                let operation =
                    FriendTcOp::SetRemoteMaxDebt(currency_config.wanted_remote_max_debt);
                queue_operation_or_cancel(
                    m_state,
                    rng,
                    pending_move_token,
                    cancel_public_keys,
                    outgoing_control,
                    &currency,
                    &operation,
                )?;
            }
        }
    }

    // Set local_requests_status (if required):
    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    for (currency, currency_config) in friend.currency_configs.clone() {
        // Set remote_max_debt and local_requests_status (if required):
        let friend = m_state.state().friends.get(friend_public_key).unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        if let Some(mutual_credit) = token_channel.get_mutual_credits().get(&currency) {
            // We need to change local requests status:
            if currency_config.wanted_local_requests_status
                != mutual_credit.state().requests_status.local
            {
                let friend_op =
                    if let RequestsStatus::Open = currency_config.wanted_local_requests_status {
                        FriendTcOp::EnableRequests
                    } else {
                        FriendTcOp::DisableRequests
                    };
                queue_operation_or_cancel(
                    m_state,
                    rng,
                    pending_move_token,
                    cancel_public_keys,
                    outgoing_control,
                    &currency,
                    &friend_op,
                )?;
            }
        }
    }

    /*
    // Set remote_max_debt if needed:
    for (currency, wanted_remote_max_debt) in channel_consistent.wanted_remote_max_debt.clone() {
        let friend = m_state.state().friends.get(friend_public_key).unwrap();
        let remote_max_debt = match &friend.channel_status {
            ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        }
        .get_remote_max_debt(&currency);

        assert_ne!(remote_max_debt, wanted_remote_max_debt);

        let operation = FriendTcOp::SetRemoteMaxDebt(wanted_remote_max_debt);
        queue_operation_or_cancel(
            m_state,
            rng,
            pending_move_token,
            cancel_public_keys,
            outgoing_control,
            &currency,
            &operation,
        )?;

        let friend_mutation = FriendMutation::ClearWantedRemoteMaxDebt(currency.clone());
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    // Open or close requests is needed:
    for (currency, wanted_local_requests_status) in channel_consistent.wanted_local_requests_status.clone() {
        let friend = m_state.state().friends.get(friend_public_key).unwrap();
        let channel_consistent = match &friend.channel_status {
            ChannelStatus::Consistent(channel_consistent) => channel_consistent,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };

        let local_requests_status = &channel_consistent
            .token_channel
            .get_mutual_credits()
            .get(&currency)
            .unwrap()
            .state()
            .requests_status
            .local;

        assert_ne!(&wanted_local_requests_status, local_requests_status);

        let friend_op = if let RequestsStatus::Open = wanted_local_requests_status {
            FriendTcOp::EnableRequests
        } else {
            FriendTcOp::DisableRequests
        };
        queue_operation_or_cancel(
            m_state,
            rng,
            pending_move_token,
            cancel_public_keys,
            outgoing_control,
            &currency,
            &friend_op,
        )?;

        let friend_mutation = FriendMutation::ClearWantedLocalRequestsStatus(currency.clone());
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }
    */

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    // Send pending responses (Response, Cancel, Collect)
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_backwards_ops = channel_consistent.pending_backwards_ops.clone();
    while let Some((currency, pending_backwards_op)) = pending_backwards_ops.pop_front() {
        let pending_op = backwards_op_to_friend_tc_op(pending_backwards_op);
        queue_operation_or_cancel(
            m_state,
            rng,
            pending_move_token,
            // Not required here, as no requests are being sent.
            cancel_public_keys,
            outgoing_control,
            &currency,
            &pending_op,
        )?;

        let friend_mutation = FriendMutation::PopFrontPendingBackwardsOp;
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    // Send pending requests:
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_requests = channel_consistent.pending_requests.clone();
    while let Some((currency, pending_request)) = pending_requests.pop_front() {
        let pending_op = FriendTcOp::RequestSendFunds(pending_request);
        queue_operation_or_cancel(
            m_state,
            rng,
            pending_move_token,
            cancel_public_keys,
            outgoing_control,
            &currency,
            &pending_op,
        )?;
        let friend_mutation = FriendMutation::PopFrontPendingRequest;
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    // Send as many pending user requests as possible:
    let mut pending_user_requests = channel_consistent.pending_user_requests.clone();
    while let Some((currency, request_send_funds)) = pending_user_requests.pop_front() {
        let pending_op = FriendTcOp::RequestSendFunds(request_send_funds);
        queue_operation_or_cancel(
            m_state,
            rng,
            pending_move_token,
            cancel_public_keys,
            outgoing_control,
            &currency,
            &pending_op,
        )?;
        let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }
    Ok(())
}

fn append_cancels_to_move_token<B, R>(
    m_state: &mut MutableFunderState<B>,
    rng: &R,
    friend_public_key: &PublicKey,
    pending_move_token: &mut PendingMoveToken<B>,
) -> Result<(), CollectOutgoingError>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
    R: CryptoRandom,
{
    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    // Send pending responses (Response, Cancel, Collect):
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_backwards_ops = channel_consistent.pending_backwards_ops.clone();
    while let Some((currency, pending_backwards_op)) = pending_backwards_ops.pop_front() {
        let pending_op = backwards_op_to_friend_tc_op(pending_backwards_op);
        // TODO: Find a more elegant way to do this:
        let mut dummy_cancel_public_keys = HashSet::new();
        let mut dummy_outgoing_control = Vec::new();
        queue_operation_or_cancel(
            m_state,
            rng,
            pending_move_token,
            &mut dummy_cancel_public_keys,
            &mut dummy_outgoing_control,
            &currency,
            &pending_op,
        )?;

        let friend_mutation = FriendMutation::PopFrontPendingBackwardsOp;
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }
    Ok(())
}

async fn send_move_token<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    friend_public_key: PublicKey,
    pending_move_token: PendingMoveToken<B>,
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
    outgoing_messages: &'a mut Vec<OutgoingMessage<B>>,
) where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
    R: CryptoRandom,
{
    let PendingMoveToken {
        pending_currencies,
        opt_local_relays,
        opt_active_currencies,
        token_wanted,
        may_send_empty,
        ..
    } = pending_move_token;

    if pending_currencies.is_empty()
        && opt_active_currencies.is_none()
        && opt_local_relays.is_none()
        && !may_send_empty
    {
        return;
    }

    // We want the token back if we just set a new address, to be sure
    // that the remote side knows about the new address.
    let token_wanted = token_wanted || opt_local_relays.is_some();

    let friend = m_state.state().friends.get(&friend_public_key).unwrap();

    let rand_nonce = RandValue::rand_gen(rng);
    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    let tc_in_borrow = &channel_consistent.token_channel.get_incoming().unwrap();

    let currencies_operations = pending_currencies
        .into_iter()
        .map(|(currency, pending_currency)| CurrencyOperations {
            currency,
            operations: pending_currency.operations,
        })
        .collect();

    // let (u_move_token, token_info) =
    let SendMoveTokenOutput {
        unsigned_move_token,
        mutations,
        token_info,
    } = tc_in_borrow
        .simulate_send_move_token(
            currencies_operations,
            opt_local_relays,
            opt_active_currencies,
            rand_nonce,
        )
        .unwrap();

    // Apply mutations:
    for tc_mutation in mutations {
        let friend_mutation = FriendMutation::TcMutation(tc_mutation);
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    // Apply final SetDirection mutation (Can not be created from inside of the TokenChannel
    // because a signature is required.
    let move_token = sign_move_token(unsigned_move_token, identity_client).await;
    let tc_mutation = TcMutation::SetDirection(SetDirection::Outgoing((move_token, token_info)));
    let friend_mutation = FriendMutation::TcMutation(tc_mutation);
    let funder_mutation =
        FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    let friend = m_state.state().friends.get(&friend_public_key).unwrap();
    let channel_consistent = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    let tc_out_borrow = &channel_consistent.token_channel.get_outgoing().unwrap();

    let move_token = tc_out_borrow.create_outgoing_move_token();
    let move_token_request = MoveTokenRequest {
        move_token,
        token_wanted,
    };

    outgoing_messages.push((
        friend_public_key.clone(),
        FriendMessage::MoveTokenRequest(move_token_request),
    ));
}

fn init_cancel_pending_move_token<B>(
    ephemeral: &Ephemeral,
    max_operations_in_batch: usize,
    cancel_public_keys: &HashSet<PublicKey>,
    pending_move_tokens: &mut HashMap<PublicKey, PendingMoveToken<B>>,
) where
    B: Clone + Eq + CanonicalSerialize + Debug,
{
    let pending_move_token_keys = pending_move_tokens.keys().cloned().collect::<HashSet<_>>();
    for friend_public_key in cancel_public_keys {
        // Make sure that this friend is ready,
        // and that it doesn't already have a PendingMoveToken:

        if !ephemeral.liveness.is_online(&friend_public_key)
            || pending_move_token_keys.contains(&friend_public_key)
        {
            continue;
        }

        let may_send_empty = false;
        let pending_move_token = PendingMoveToken::new(
            friend_public_key.clone(),
            max_operations_in_batch,
            may_send_empty,
        );
        pending_move_tokens.insert(friend_public_key.clone(), pending_move_token);
    }
}

/// Send all possible messages according to SendCommands
pub async fn create_friend_messages<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    ephemeral: &'a Ephemeral,
    send_commands: &'a SendCommands,
    max_operations_in_batch: usize,
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
) -> (
    Vec<FunderOutgoingControl<B>>,
    Vec<OutgoingMessage<B>>,
    Vec<ChannelerConfig<RelayAddress<B>>>,
)
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug + Hash,
    R: CryptoRandom,
{
    let mut outgoing_control = Vec::new();
    let mut outgoing_messages = Vec::new();
    let mut outgoing_channeler_config = Vec::new();
    let mut pending_move_tokens: HashMap<PublicKey, PendingMoveToken<B>> = HashMap::new();

    // First iteration:
    let mut cancel_public_keys = HashSet::new();
    for (friend_public_key, friend_send_commands) in &send_commands.send_commands {
        if !ephemeral.liveness.is_online(friend_public_key) {
            continue;
        }
        send_friend_iter1(
            m_state,
            friend_public_key,
            friend_send_commands,
            &mut pending_move_tokens,
            identity_client,
            rng,
            max_operations_in_batch,
            &mut cancel_public_keys,
            &mut outgoing_messages,
            &mut outgoing_control,
            &mut outgoing_channeler_config,
        )
        .await;
    }

    // Create PendingMoveToken-s for all the friends that were queued
    // new pending messages during `send_friend_iter1`:
    init_cancel_pending_move_token(
        ephemeral,
        max_operations_in_batch,
        &cancel_public_keys,
        &mut pending_move_tokens,
    );

    // Second iteration (Attempt to queue Cancel-s created in the first iteration):
    for (friend_public_key, pending_move_token) in &mut pending_move_tokens {
        assert!(ephemeral.liveness.is_online(&friend_public_key));
        let _ = append_cancels_to_move_token(m_state, rng, friend_public_key, pending_move_token);
    }

    // Send all pending move tokens:
    for (friend_public_key, pending_move_token) in pending_move_tokens.into_iter() {
        assert!(ephemeral.liveness.is_online(&friend_public_key));
        send_move_token(
            m_state,
            friend_public_key,
            pending_move_token,
            identity_client,
            rng,
            &mut outgoing_messages,
        )
        .await;
    }

    (
        outgoing_control,
        outgoing_messages,
        outgoing_channeler_config,
    )
}
