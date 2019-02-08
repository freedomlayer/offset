use capnp;
use capnp::serialize_packed;
use crypto::identity::PublicKey;
use common::int_convert::usize_to_u32;
use crate::capnp_common::{write_signature, read_signature,
                          write_custom_int128, read_custom_int128,
                          write_custom_u_int128, read_custom_u_int128,
                          write_rand_nonce, read_rand_nonce,
                          write_uid, read_uid,
                          write_invoice_id, read_invoice_id,
                          write_public_key, read_public_key,
                          write_relay_address, read_relay_address,
                          write_index_server_address, read_index_server_address,
                          write_receipt, read_receipt,
                          write_hash, read_hash};

use report_capnp;
use crate::serialize::SerializeError;
use crate::report::messages::{MoveTokenHashedReport, FriendStatusReport, RequestsStatusReport,
                            FriendLivenessReport, DirectionReport,
                            McRequestsStatusReport, McBalanceReport, TcReport,
                            ResetTermsReport, ChannelInconsistentReport,
                            ChannelStatusReport, FriendReport,
                            SentLocalAddressReport};

use crate::funder::messages::RelayAddress;


fn ser_move_token_hashed_report(move_token_hashed_report: &MoveTokenHashedReport,
                    move_token_hashed_report_builder: &mut report_capnp::move_token_hashed_report::Builder) {

    write_hash(&move_token_hashed_report.prefix_hash,
        &mut move_token_hashed_report_builder.reborrow().init_prefix_hash());

    write_public_key(&move_token_hashed_report.local_public_key,
        &mut move_token_hashed_report_builder.reborrow().init_local_public_key());

    write_public_key(&move_token_hashed_report.remote_public_key,
        &mut move_token_hashed_report_builder.reborrow().init_remote_public_key());

    move_token_hashed_report_builder.reborrow().set_inconsistency_counter(move_token_hashed_report.inconsistency_counter);

    write_custom_u_int128(move_token_hashed_report.move_token_counter,
        &mut move_token_hashed_report_builder.reborrow().init_move_token_counter());

    write_custom_int128(move_token_hashed_report.balance,
        &mut move_token_hashed_report_builder.reborrow().init_balance());

    write_custom_u_int128(move_token_hashed_report.local_pending_debt,
        &mut move_token_hashed_report_builder.reborrow().init_local_pending_debt());

    write_custom_u_int128(move_token_hashed_report.remote_pending_debt,
        &mut move_token_hashed_report_builder.reborrow().init_remote_pending_debt());

    write_rand_nonce(&move_token_hashed_report.rand_nonce,
        &mut move_token_hashed_report_builder.reborrow().init_rand_nonce());

    write_signature(&move_token_hashed_report.new_token,
        &mut move_token_hashed_report_builder.reborrow().init_new_token());
}

fn deser_move_token_hashed_report(move_token_hashed_report_reader: &report_capnp::move_token_hashed_report::Reader)
    -> Result<MoveTokenHashedReport, SerializeError> {

    Ok(MoveTokenHashedReport {
        prefix_hash: read_hash(&move_token_hashed_report_reader.get_prefix_hash()?)?,
        local_public_key: read_public_key(&move_token_hashed_report_reader.get_local_public_key()?)?,
        remote_public_key: read_public_key(&move_token_hashed_report_reader.get_remote_public_key()?)?,
        inconsistency_counter: move_token_hashed_report_reader.get_inconsistency_counter(),
        move_token_counter: read_custom_u_int128(&move_token_hashed_report_reader.get_move_token_counter()?)?,
        balance: read_custom_int128(&move_token_hashed_report_reader.get_balance()?)?,
        local_pending_debt: read_custom_u_int128(&move_token_hashed_report_reader.get_local_pending_debt()?)?,
        remote_pending_debt: read_custom_u_int128(&move_token_hashed_report_reader.get_remote_pending_debt()?)?,
        rand_nonce: read_rand_nonce(&move_token_hashed_report_reader.get_rand_nonce()?)?,
        new_token: read_signature(&move_token_hashed_report_reader.get_new_token()?)?,
    })
}

fn ser_friend_status_report(friend_status_report: &FriendStatusReport,
                    friend_status_report_builder: &mut report_capnp::friend_status_report::Builder) {

    match friend_status_report {
        FriendStatusReport::Enabled => friend_status_report_builder.set_enabled(()),
        FriendStatusReport::Disabled => friend_status_report_builder.set_disabled(()),
    }
}

fn deser_friend_status_report(friend_status_report_reader: &report_capnp::friend_status_report::Reader)
    -> Result<FriendStatusReport, SerializeError> {

    Ok(match friend_status_report_reader.which()? {
        report_capnp::friend_status_report::Disabled(()) => FriendStatusReport::Disabled,
        report_capnp::friend_status_report::Enabled(()) => FriendStatusReport::Enabled,
    })
}

fn ser_requests_status_report(requests_status_report: &RequestsStatusReport,
                    requests_status_report_builder: &mut report_capnp::requests_status_report::Builder) {

    match requests_status_report {
        RequestsStatusReport::Closed => requests_status_report_builder.set_closed(()),
        RequestsStatusReport::Open => requests_status_report_builder.set_open(()),
    }
}

fn deser_requests_status_report(requests_status_report_reader: &report_capnp::requests_status_report::Reader)
    -> Result<RequestsStatusReport, SerializeError> {

    Ok(match requests_status_report_reader.which()? {
        report_capnp::requests_status_report::Closed(()) => RequestsStatusReport::Closed,
        report_capnp::requests_status_report::Open(()) => RequestsStatusReport::Open,
    })
}

fn ser_friend_liveness_report(friend_liveness_report: &FriendLivenessReport,
                    friend_liveness_report_builder: &mut report_capnp::friend_liveness_report::Builder) {

    match friend_liveness_report {
        FriendLivenessReport::Offline => friend_liveness_report_builder.set_offline(()),
        FriendLivenessReport::Online => friend_liveness_report_builder.set_online(()),
    }
}

fn deser_friend_liveness_report(friend_liveness_report_reader: &report_capnp::friend_liveness_report::Reader)
    -> Result<FriendLivenessReport, SerializeError> {

    Ok(match friend_liveness_report_reader.which()? {
        report_capnp::friend_liveness_report::Offline(()) => FriendLivenessReport::Offline,
        report_capnp::friend_liveness_report::Online(()) => FriendLivenessReport::Online,
    })
}

fn ser_direction_report(direction_report: &DirectionReport,
                    direction_report_builder: &mut report_capnp::direction_report::Builder) {

    match direction_report {
        DirectionReport::Incoming => direction_report_builder.set_incoming(()),
        DirectionReport::Outgoing => direction_report_builder.set_outgoing(()),
    }
}

fn deser_direction_report(direction_report_reader: &report_capnp::direction_report::Reader)
    -> Result<DirectionReport, SerializeError> {

    Ok(match direction_report_reader.which()? {
        report_capnp::direction_report::Incoming(()) => DirectionReport::Incoming,
        report_capnp::direction_report::Outgoing(()) => DirectionReport::Outgoing,
    })
}

fn ser_mc_requests_status_report(mc_requests_status_report: &McRequestsStatusReport,
                    mc_requests_status_report_builder: &mut report_capnp::mc_requests_status_report::Builder) {

    ser_requests_status_report(&mc_requests_status_report.local, 
            &mut mc_requests_status_report_builder.reborrow().init_local());

    ser_requests_status_report(&mc_requests_status_report.remote, 
            &mut mc_requests_status_report_builder.reborrow().init_remote());

}

fn deser_mc_requests_status_report(mc_requests_status_report: &report_capnp::mc_requests_status_report::Reader)
    -> Result<McRequestsStatusReport, SerializeError> {

    Ok(McRequestsStatusReport {
        local: deser_requests_status_report(&mc_requests_status_report.get_local()?)?,
        remote: deser_requests_status_report(&mc_requests_status_report.get_remote()?)?,
    })
}

fn ser_mc_balance_report(mc_balance_report: &McBalanceReport,
                    mc_balance_report_builder: &mut report_capnp::mc_balance_report::Builder) {

    write_custom_int128(mc_balance_report.balance,
        &mut mc_balance_report_builder.reborrow().init_balance());

    write_custom_u_int128(mc_balance_report.local_max_debt,
        &mut mc_balance_report_builder.reborrow().init_local_max_debt());

    write_custom_u_int128(mc_balance_report.remote_max_debt,
        &mut mc_balance_report_builder.reborrow().init_remote_max_debt());

    write_custom_u_int128(mc_balance_report.local_pending_debt,
        &mut mc_balance_report_builder.reborrow().init_local_pending_debt());

    write_custom_u_int128(mc_balance_report.remote_pending_debt,
        &mut mc_balance_report_builder.reborrow().init_remote_pending_debt());
}

fn deser_mc_balance_report(mc_balance_report_reader: &report_capnp::mc_balance_report::Reader)
    -> Result<McBalanceReport, SerializeError> {

    Ok(McBalanceReport {
        balance: read_custom_int128(&mc_balance_report_reader.get_balance()?)?,
        local_max_debt: read_custom_u_int128(&mc_balance_report_reader.get_local_max_debt()?)?,
        remote_max_debt: read_custom_u_int128(&mc_balance_report_reader.get_remote_max_debt()?)?,
        local_pending_debt: read_custom_u_int128(&mc_balance_report_reader.get_local_pending_debt()?)?,
        remote_pending_debt: read_custom_u_int128(&mc_balance_report_reader.get_remote_pending_debt()?)?,
    })
}

fn ser_tc_report(tc_report: &TcReport,
                    tc_report_builder: &mut report_capnp::tc_report::Builder) {

    ser_direction_report(&tc_report.direction, 
            &mut tc_report_builder.reborrow().init_direction());

    ser_mc_balance_report(&tc_report.balance, 
            &mut tc_report_builder.reborrow().init_balance());

    ser_mc_requests_status_report(&tc_report.requests_status, 
            &mut tc_report_builder.reborrow().init_requests_status());

    tc_report_builder.reborrow().set_num_local_pending_requests(tc_report.num_local_pending_requests);
    tc_report_builder.reborrow().set_num_remote_pending_requests(tc_report.num_remote_pending_requests);
}

fn deser_tc_report(tc_report_reader: &report_capnp::tc_report::Reader)
    -> Result<TcReport, SerializeError> {

    Ok(TcReport {
        direction: deser_direction_report(&tc_report_reader.get_direction()?)?,
        balance: deser_mc_balance_report(&tc_report_reader.get_balance()?)?,
        requests_status: deser_mc_requests_status_report(&tc_report_reader.get_requests_status()?)?,
        num_local_pending_requests: tc_report_reader.get_num_local_pending_requests(),
        num_remote_pending_requests: tc_report_reader.get_num_remote_pending_requests(),
    })
}

fn ser_reset_terms_report(reset_terms_report: &ResetTermsReport,
                    reset_terms_report_builder: &mut report_capnp::reset_terms_report::Builder) {

    write_signature(&reset_terms_report.reset_token,
                    &mut reset_terms_report_builder.reborrow().init_reset_token());

    write_custom_int128(reset_terms_report.balance_for_reset,
                    &mut reset_terms_report_builder.reborrow().init_balance_for_reset());
}

fn deser_reset_terms_report(reset_terms_report_reader: &report_capnp::reset_terms_report::Reader)
    -> Result<ResetTermsReport, SerializeError> {

    Ok(ResetTermsReport {
        reset_token: read_signature(&reset_terms_report_reader.get_reset_token()?)?,
        balance_for_reset: read_custom_int128(&reset_terms_report_reader.get_balance_for_reset()?)?,
    })
}

fn ser_channel_inconsistent_report(channel_inconsistent_report: &ChannelInconsistentReport,
                    channel_inconsistent_report_builder: &mut report_capnp::channel_inconsistent_report::Builder) {

    write_custom_int128(channel_inconsistent_report.local_reset_terms_balance, 
            &mut channel_inconsistent_report_builder.reborrow().init_local_reset_terms_balance());

    let mut opt_remote_reset_terms_builder = channel_inconsistent_report_builder
        .reborrow()
        .init_opt_remote_reset_terms();
    match &channel_inconsistent_report.opt_remote_reset_terms {
        Some(remote_reset_terms) => {
            let mut remote_reset_terms_builder = opt_remote_reset_terms_builder
                .reborrow().init_remote_reset_terms();
            ser_reset_terms_report(remote_reset_terms,
                                &mut remote_reset_terms_builder);

        },
        None => {opt_remote_reset_terms_builder.reborrow().set_empty(());}
    };
}

fn deser_channel_inconsistent_report(channel_inconsistent_report_reader: &report_capnp::channel_inconsistent_report::Reader)
    -> Result<ChannelInconsistentReport, SerializeError> {

    let opt_remote_reset_terms = match channel_inconsistent_report_reader.get_opt_remote_reset_terms().which()? {
        report_capnp::channel_inconsistent_report
            ::opt_remote_reset_terms
            ::RemoteResetTerms(reset_terms_report_reader) => Some(deser_reset_terms_report(&reset_terms_report_reader?)?),
        report_capnp
            ::channel_inconsistent_report
            ::opt_remote_reset_terms
            ::Empty(()) => None,
    };

    Ok(ChannelInconsistentReport {
        local_reset_terms_balance: read_custom_int128(&channel_inconsistent_report_reader.get_local_reset_terms_balance()?)?,
        opt_remote_reset_terms,
    })
}

fn ser_channel_status_report(channel_status_report: &ChannelStatusReport,
                    channel_status_report_builder: &mut report_capnp::channel_status_report::Builder) {

    match channel_status_report {
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            let mut inconsistent_builder = channel_status_report_builder.reborrow().init_inconsistent();
            ser_channel_inconsistent_report(channel_inconsistent_report,
                                            &mut inconsistent_builder);
        },
        ChannelStatusReport::Consistent(tc_report) => {
            let mut consistent_builder = channel_status_report_builder.reborrow().init_consistent();
            ser_tc_report(tc_report,
                          &mut consistent_builder);
        },
    };
}

fn deser_channel_status_report(channel_status_report_reader: &report_capnp::channel_status_report::Reader)
    -> Result<ChannelStatusReport, SerializeError> {

    Ok(match channel_status_report_reader.which()? {
        report_capnp::channel_status_report::Inconsistent(channel_inconsistent_report_reader) =>
            ChannelStatusReport::Inconsistent(
                deser_channel_inconsistent_report(&channel_inconsistent_report_reader?)?),
        report_capnp::channel_status_report::Consistent(tc_report_reader) => 
            ChannelStatusReport::Consistent(
                deser_tc_report(&tc_report_reader?)?),
    })
}

fn ser_opt_last_incoming_move_token(opt_last_incoming_move_token: &Option<MoveTokenHashedReport>,
                    opt_last_incoming_move_token_builder: &mut report_capnp::opt_last_incoming_move_token::Builder) {

    match opt_last_incoming_move_token {
        Some(last_incoming_move_token) => {
            let mut move_token_hashed_builder = 
                opt_last_incoming_move_token_builder.reborrow().init_move_token_hashed();

            ser_move_token_hashed_report(last_incoming_move_token,
                                  &mut move_token_hashed_builder);
        },
        None => {
            opt_last_incoming_move_token_builder.set_empty(());
        },
    };
}

fn deser_opt_last_incoming_move_token(opt_last_incoming_move_token_reader: &report_capnp::opt_last_incoming_move_token::Reader)
    -> Result<Option<MoveTokenHashedReport>, SerializeError> {

    Ok(match opt_last_incoming_move_token_reader.which()? {
        report_capnp::opt_last_incoming_move_token::MoveTokenHashed(move_token_hashed_reader) =>
            Some(deser_move_token_hashed_report(&move_token_hashed_reader?)?),
        report_capnp::opt_last_incoming_move_token::Empty(()) => None,
    })
}

fn ser_relays_transition(relays_transition: &(Vec<RelayAddress>, Vec<RelayAddress>),
                    relays_transition_builder: &mut report_capnp::relays_transition::Builder) {

    let (last_sent, before_last_sent) = relays_transition;

    let mut last_sent_builder = relays_transition_builder
        .reborrow()
        .init_last_sent(usize_to_u32(last_sent.len()).unwrap());

    for (index, relay_address) in last_sent.iter().enumerate() {
        let mut relay_address_builder = last_sent_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_relay_address(relay_address, &mut relay_address_builder);
    }

    let mut before_last_sent_builder = relays_transition_builder
        .reborrow()
        .init_before_last_sent(usize_to_u32(last_sent.len()).unwrap());

    for (index, relay_address) in before_last_sent.iter().enumerate() {
        let mut relay_address_builder = before_last_sent_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_relay_address(relay_address, &mut relay_address_builder);
    }
}

fn deser_relays_transition(relays_transition_reader: &report_capnp::relays_transition::Reader)
    -> Result<(Vec<RelayAddress>, Vec<RelayAddress>), SerializeError> {

    let mut last_sent = Vec::new();
    for relay_address in relays_transition_reader.get_last_sent()? {
        last_sent.push(read_relay_address(&relay_address)?);
    }

    let mut before_last_sent = Vec::new();
    for relay_address in relays_transition_reader.get_before_last_sent()? {
        before_last_sent.push(read_relay_address(&relay_address)?);
    }

    Ok((last_sent, before_last_sent))
}

fn ser_sent_local_relays_report(sent_local_address_report: &SentLocalAddressReport<Vec<RelayAddress>>,
                    sent_local_relays_report_builder: &mut report_capnp::sent_local_relays_report::Builder) {

    match sent_local_address_report {
        SentLocalAddressReport::NeverSent => {
            sent_local_relays_report_builder.set_never_sent(());
        },
        SentLocalAddressReport::Transition(relays_transition) => {
            let mut relays_transition_builder = sent_local_relays_report_builder.reborrow().init_transition();
            ser_relays_transition(&relays_transition, &mut relays_transition_builder);
        },
        SentLocalAddressReport::LastSent(last_sent) => {
            let relays_len = usize_to_u32(last_sent.len()).unwrap();
            let mut last_sent_builder = sent_local_relays_report_builder.reborrow().init_last_sent(relays_len);
            for (index, relay_address) in last_sent.iter().enumerate() {
                let mut relay_address_builder = last_sent_builder.reborrow().get(usize_to_u32(index).unwrap());
                write_relay_address(relay_address, &mut relay_address_builder);
            }
        },
    }
}

fn deser_sent_local_relays_report(sent_local_relays_report_reader: &report_capnp::sent_local_relays_report::Reader)
    -> Result<SentLocalAddressReport<Vec<RelayAddress>>, SerializeError> {

    Ok(match sent_local_relays_report_reader.which()? {
        report_capnp::sent_local_relays_report::NeverSent(()) => SentLocalAddressReport::NeverSent,
        report_capnp::sent_local_relays_report::Transition(relays_transition_reader) =>
            SentLocalAddressReport::Transition(deser_relays_transition(&relays_transition_reader?)?),
        report_capnp::sent_local_relays_report::LastSent(last_sent_reader) => {
            let mut last_sent = Vec::new();
            for relay_address in last_sent_reader? {
                last_sent.push(read_relay_address(&relay_address)?);
            }
            SentLocalAddressReport::LastSent(last_sent)
        },
    })
}


fn ser_friend_report(friend_report: &FriendReport<Vec<RelayAddress>>,
                    friend_report_builder: &mut report_capnp::friend_report::Builder) {

    friend_report_builder.reborrow().set_name(&friend_report.name);

    // remote_relays:
    let relays_len = usize_to_u32(friend_report.remote_address.len()).unwrap();
    let mut relays_builder = friend_report_builder.reborrow().init_remote_relays(relays_len);
    for (index, relay_address) in friend_report.remote_address.iter().enumerate() {
        let mut relay_address_builder = relays_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_relay_address(relay_address, &mut relay_address_builder);
    }

    ser_sent_local_relays_report(&friend_report.sent_local_address,
        &mut friend_report_builder.reborrow().init_sent_local_relays());

    ser_opt_last_incoming_move_token(&friend_report.opt_last_incoming_move_token,
        &mut friend_report_builder.reborrow().init_opt_last_incoming_move_token());

    ser_friend_liveness_report(&friend_report.liveness,
        &mut friend_report_builder.reborrow().init_liveness());

    ser_channel_status_report(&friend_report.channel_status,
        &mut friend_report_builder.reborrow().init_channel_status());

    write_custom_u_int128(friend_report.wanted_remote_max_debt,
        &mut friend_report_builder.reborrow().init_wanted_remote_max_debt());

    ser_requests_status_report(&friend_report.wanted_local_requests_status,
        &mut friend_report_builder.reborrow().init_wanted_local_requests_status());


    friend_report_builder.set_num_pending_requests(friend_report.num_pending_requests);
    friend_report_builder.set_num_pending_responses(friend_report.num_pending_responses);

    ser_friend_status_report(&friend_report.status, 
                        &mut friend_report_builder.reborrow().init_status());

    friend_report_builder.set_num_pending_user_requests(friend_report.num_pending_user_requests);
}

fn deser_friend_report(friend_report_reader: &report_capnp::friend_report::Reader)
    -> Result<FriendReport<Vec<RelayAddress>>, SerializeError> {

    let mut remote_address = Vec::new();
    for relay_address in friend_report_reader.get_remote_relays()? {
        remote_address.push(read_relay_address(&relay_address)?);
    }

    Ok(FriendReport {
        name: friend_report_reader.get_name()?.to_owned(),
        remote_address,
        sent_local_address: deser_sent_local_relays_report(&friend_report_reader.get_sent_local_relays()?)?,
        opt_last_incoming_move_token: deser_opt_last_incoming_move_token(
            &friend_report_reader.get_opt_last_incoming_move_token()?)?,
        liveness: deser_friend_liveness_report(&friend_report_reader.get_liveness()?)?,
        channel_status: deser_channel_status_report(&friend_report_reader.get_channel_status()?)?,
        wanted_remote_max_debt: read_custom_u_int128(&friend_report_reader.get_wanted_remote_max_debt()?)?,
        wanted_local_requests_status: deser_requests_status_report(&friend_report_reader.get_wanted_local_requests_status()?)?,
        num_pending_requests: friend_report_reader.get_num_pending_requests(),
        num_pending_responses: friend_report_reader.get_num_pending_responses(),
        status: deser_friend_status_report(&friend_report_reader.get_status()?)?,
        num_pending_user_requests: friend_report_reader.get_num_pending_user_requests(),
    })
}
