use im::hashmap::HashMap as ImHashMap;
use im::vector::Vector as ImVec;

use crate::capnp_common::{
    read_custom_int128, read_custom_u_int128, read_hash, read_named_index_server_address,
    read_named_relay_address, read_public_key, read_rand_nonce, read_rate, read_relay_address,
    read_signature, write_custom_int128, write_custom_u_int128, write_hash,
    write_named_index_server_address, write_named_relay_address, write_public_key,
    write_rand_nonce, write_rate, write_relay_address, write_signature,
};
use common::int_convert::usize_to_u32;
use crypto::identity::PublicKey;

use crate::report::messages::{
    AddFriendReport, ChannelInconsistentReport, ChannelStatusReport, DirectionReport,
    FriendLivenessReport, FriendReport, FriendReportMutation, FriendStatusReport, FunderReport,
    FunderReportMutation, McBalanceReport, McRequestsStatusReport, MoveTokenHashedReport,
    RequestsStatusReport, ResetTermsReport, SentLocalRelaysReport, TcReport,
};
use crate::serialize::SerializeError;
use report_capnp;

use crate::app_server::messages::NamedRelayAddress;
use crate::app_server::messages::{NodeReport, NodeReportMutation};
use crate::index_client::messages::{IndexClientReport, IndexClientReportMutation};
use crate::net::messages::NetAddress;

fn ser_move_token_hashed_report(
    move_token_hashed_report: &MoveTokenHashedReport,
    move_token_hashed_report_builder: &mut report_capnp::move_token_hashed_report::Builder,
) {
    write_hash(
        &move_token_hashed_report.prefix_hash,
        &mut move_token_hashed_report_builder
            .reborrow()
            .init_prefix_hash(),
    );

    write_public_key(
        &move_token_hashed_report.local_public_key,
        &mut move_token_hashed_report_builder
            .reborrow()
            .init_local_public_key(),
    );

    write_public_key(
        &move_token_hashed_report.remote_public_key,
        &mut move_token_hashed_report_builder
            .reborrow()
            .init_remote_public_key(),
    );

    move_token_hashed_report_builder
        .reborrow()
        .set_inconsistency_counter(move_token_hashed_report.inconsistency_counter);

    write_custom_u_int128(
        move_token_hashed_report.move_token_counter,
        &mut move_token_hashed_report_builder
            .reborrow()
            .init_move_token_counter(),
    );

    write_custom_int128(
        move_token_hashed_report.balance,
        &mut move_token_hashed_report_builder.reborrow().init_balance(),
    );

    write_custom_u_int128(
        move_token_hashed_report.local_pending_debt,
        &mut move_token_hashed_report_builder
            .reborrow()
            .init_local_pending_debt(),
    );

    write_custom_u_int128(
        move_token_hashed_report.remote_pending_debt,
        &mut move_token_hashed_report_builder
            .reborrow()
            .init_remote_pending_debt(),
    );

    write_rand_nonce(
        &move_token_hashed_report.rand_nonce,
        &mut move_token_hashed_report_builder
            .reborrow()
            .init_rand_nonce(),
    );

    write_signature(
        &move_token_hashed_report.new_token,
        &mut move_token_hashed_report_builder.reborrow().init_new_token(),
    );
}

fn deser_move_token_hashed_report(
    move_token_hashed_report_reader: &report_capnp::move_token_hashed_report::Reader,
) -> Result<MoveTokenHashedReport, SerializeError> {
    Ok(MoveTokenHashedReport {
        prefix_hash: read_hash(&move_token_hashed_report_reader.get_prefix_hash()?)?,
        local_public_key: read_public_key(
            &move_token_hashed_report_reader.get_local_public_key()?,
        )?,
        remote_public_key: read_public_key(
            &move_token_hashed_report_reader.get_remote_public_key()?,
        )?,
        inconsistency_counter: move_token_hashed_report_reader.get_inconsistency_counter(),
        move_token_counter: read_custom_u_int128(
            &move_token_hashed_report_reader.get_move_token_counter()?,
        )?,
        balance: read_custom_int128(&move_token_hashed_report_reader.get_balance()?)?,
        local_pending_debt: read_custom_u_int128(
            &move_token_hashed_report_reader.get_local_pending_debt()?,
        )?,
        remote_pending_debt: read_custom_u_int128(
            &move_token_hashed_report_reader.get_remote_pending_debt()?,
        )?,
        rand_nonce: read_rand_nonce(&move_token_hashed_report_reader.get_rand_nonce()?)?,
        new_token: read_signature(&move_token_hashed_report_reader.get_new_token()?)?,
    })
}

fn ser_friend_status_report(
    friend_status_report: &FriendStatusReport,
    friend_status_report_builder: &mut report_capnp::friend_status_report::Builder,
) {
    match friend_status_report {
        FriendStatusReport::Enabled => friend_status_report_builder.set_enabled(()),
        FriendStatusReport::Disabled => friend_status_report_builder.set_disabled(()),
    }
}

fn deser_friend_status_report(
    friend_status_report_reader: &report_capnp::friend_status_report::Reader,
) -> Result<FriendStatusReport, SerializeError> {
    Ok(match friend_status_report_reader.which()? {
        report_capnp::friend_status_report::Disabled(()) => FriendStatusReport::Disabled,
        report_capnp::friend_status_report::Enabled(()) => FriendStatusReport::Enabled,
    })
}

fn ser_requests_status_report(
    requests_status_report: &RequestsStatusReport,
    requests_status_report_builder: &mut report_capnp::requests_status_report::Builder,
) {
    match requests_status_report {
        RequestsStatusReport::Closed => requests_status_report_builder.set_closed(()),
        RequestsStatusReport::Open => requests_status_report_builder.set_open(()),
    }
}

fn deser_requests_status_report(
    requests_status_report_reader: &report_capnp::requests_status_report::Reader,
) -> Result<RequestsStatusReport, SerializeError> {
    Ok(match requests_status_report_reader.which()? {
        report_capnp::requests_status_report::Closed(()) => RequestsStatusReport::Closed,
        report_capnp::requests_status_report::Open(()) => RequestsStatusReport::Open,
    })
}

fn ser_friend_liveness_report(
    friend_liveness_report: &FriendLivenessReport,
    friend_liveness_report_builder: &mut report_capnp::friend_liveness_report::Builder,
) {
    match friend_liveness_report {
        FriendLivenessReport::Offline => friend_liveness_report_builder.set_offline(()),
        FriendLivenessReport::Online => friend_liveness_report_builder.set_online(()),
    }
}

fn deser_friend_liveness_report(
    friend_liveness_report_reader: &report_capnp::friend_liveness_report::Reader,
) -> Result<FriendLivenessReport, SerializeError> {
    Ok(match friend_liveness_report_reader.which()? {
        report_capnp::friend_liveness_report::Offline(()) => FriendLivenessReport::Offline,
        report_capnp::friend_liveness_report::Online(()) => FriendLivenessReport::Online,
    })
}

fn ser_direction_report(
    direction_report: &DirectionReport,
    direction_report_builder: &mut report_capnp::direction_report::Builder,
) {
    match direction_report {
        DirectionReport::Incoming => direction_report_builder.set_incoming(()),
        DirectionReport::Outgoing => direction_report_builder.set_outgoing(()),
    }
}

fn deser_direction_report(
    direction_report_reader: &report_capnp::direction_report::Reader,
) -> Result<DirectionReport, SerializeError> {
    Ok(match direction_report_reader.which()? {
        report_capnp::direction_report::Incoming(()) => DirectionReport::Incoming,
        report_capnp::direction_report::Outgoing(()) => DirectionReport::Outgoing,
    })
}

fn ser_mc_requests_status_report(
    mc_requests_status_report: &McRequestsStatusReport,
    mc_requests_status_report_builder: &mut report_capnp::mc_requests_status_report::Builder,
) {
    ser_requests_status_report(
        &mc_requests_status_report.local,
        &mut mc_requests_status_report_builder.reborrow().init_local(),
    );

    ser_requests_status_report(
        &mc_requests_status_report.remote,
        &mut mc_requests_status_report_builder.reborrow().init_remote(),
    );
}

fn deser_mc_requests_status_report(
    mc_requests_status_report: &report_capnp::mc_requests_status_report::Reader,
) -> Result<McRequestsStatusReport, SerializeError> {
    Ok(McRequestsStatusReport {
        local: deser_requests_status_report(&mc_requests_status_report.get_local()?)?,
        remote: deser_requests_status_report(&mc_requests_status_report.get_remote()?)?,
    })
}

fn ser_mc_balance_report(
    mc_balance_report: &McBalanceReport,
    mc_balance_report_builder: &mut report_capnp::mc_balance_report::Builder,
) {
    write_custom_int128(
        mc_balance_report.balance,
        &mut mc_balance_report_builder.reborrow().init_balance(),
    );

    write_custom_u_int128(
        mc_balance_report.local_max_debt,
        &mut mc_balance_report_builder.reborrow().init_local_max_debt(),
    );

    write_custom_u_int128(
        mc_balance_report.remote_max_debt,
        &mut mc_balance_report_builder.reborrow().init_remote_max_debt(),
    );

    write_custom_u_int128(
        mc_balance_report.local_pending_debt,
        &mut mc_balance_report_builder
            .reborrow()
            .init_local_pending_debt(),
    );

    write_custom_u_int128(
        mc_balance_report.remote_pending_debt,
        &mut mc_balance_report_builder
            .reborrow()
            .init_remote_pending_debt(),
    );
}

fn deser_mc_balance_report(
    mc_balance_report_reader: &report_capnp::mc_balance_report::Reader,
) -> Result<McBalanceReport, SerializeError> {
    Ok(McBalanceReport {
        balance: read_custom_int128(&mc_balance_report_reader.get_balance()?)?,
        local_max_debt: read_custom_u_int128(&mc_balance_report_reader.get_local_max_debt()?)?,
        remote_max_debt: read_custom_u_int128(&mc_balance_report_reader.get_remote_max_debt()?)?,
        local_pending_debt: read_custom_u_int128(
            &mc_balance_report_reader.get_local_pending_debt()?,
        )?,
        remote_pending_debt: read_custom_u_int128(
            &mc_balance_report_reader.get_remote_pending_debt()?,
        )?,
    })
}

fn ser_tc_report(tc_report: &TcReport, tc_report_builder: &mut report_capnp::tc_report::Builder) {
    ser_direction_report(
        &tc_report.direction,
        &mut tc_report_builder.reborrow().init_direction(),
    );

    ser_mc_balance_report(
        &tc_report.balance,
        &mut tc_report_builder.reborrow().init_balance(),
    );

    ser_mc_requests_status_report(
        &tc_report.requests_status,
        &mut tc_report_builder.reborrow().init_requests_status(),
    );

    tc_report_builder
        .reborrow()
        .set_num_local_pending_requests(tc_report.num_local_pending_requests);
    tc_report_builder
        .reborrow()
        .set_num_remote_pending_requests(tc_report.num_remote_pending_requests);
}

fn deser_tc_report(
    tc_report_reader: &report_capnp::tc_report::Reader,
) -> Result<TcReport, SerializeError> {
    Ok(TcReport {
        direction: deser_direction_report(&tc_report_reader.get_direction()?)?,
        balance: deser_mc_balance_report(&tc_report_reader.get_balance()?)?,
        requests_status: deser_mc_requests_status_report(&tc_report_reader.get_requests_status()?)?,
        num_local_pending_requests: tc_report_reader.get_num_local_pending_requests(),
        num_remote_pending_requests: tc_report_reader.get_num_remote_pending_requests(),
    })
}

fn ser_reset_terms_report(
    reset_terms_report: &ResetTermsReport,
    reset_terms_report_builder: &mut report_capnp::reset_terms_report::Builder,
) {
    write_signature(
        &reset_terms_report.reset_token,
        &mut reset_terms_report_builder.reborrow().init_reset_token(),
    );

    write_custom_int128(
        reset_terms_report.balance_for_reset,
        &mut reset_terms_report_builder
            .reborrow()
            .init_balance_for_reset(),
    );
}

fn deser_reset_terms_report(
    reset_terms_report_reader: &report_capnp::reset_terms_report::Reader,
) -> Result<ResetTermsReport, SerializeError> {
    Ok(ResetTermsReport {
        reset_token: read_signature(&reset_terms_report_reader.get_reset_token()?)?,
        balance_for_reset: read_custom_int128(&reset_terms_report_reader.get_balance_for_reset()?)?,
    })
}

fn ser_channel_inconsistent_report(
    channel_inconsistent_report: &ChannelInconsistentReport,
    channel_inconsistent_report_builder: &mut report_capnp::channel_inconsistent_report::Builder,
) {
    write_custom_int128(
        channel_inconsistent_report.local_reset_terms_balance,
        &mut channel_inconsistent_report_builder
            .reborrow()
            .init_local_reset_terms_balance(),
    );

    let mut opt_remote_reset_terms_builder = channel_inconsistent_report_builder
        .reborrow()
        .init_opt_remote_reset_terms();
    match &channel_inconsistent_report.opt_remote_reset_terms {
        Some(remote_reset_terms) => {
            let mut remote_reset_terms_builder = opt_remote_reset_terms_builder
                .reborrow()
                .init_remote_reset_terms();
            ser_reset_terms_report(remote_reset_terms, &mut remote_reset_terms_builder);
        }
        None => {
            opt_remote_reset_terms_builder.reborrow().set_empty(());
        }
    };
}

fn deser_channel_inconsistent_report(
    channel_inconsistent_report_reader: &report_capnp::channel_inconsistent_report::Reader,
) -> Result<ChannelInconsistentReport, SerializeError> {
    let opt_remote_reset_terms = match channel_inconsistent_report_reader
        .get_opt_remote_reset_terms()
        .which()?
    {
        report_capnp::channel_inconsistent_report::opt_remote_reset_terms::RemoteResetTerms(
            reset_terms_report_reader,
        ) => Some(deser_reset_terms_report(&reset_terms_report_reader?)?),
        report_capnp::channel_inconsistent_report::opt_remote_reset_terms::Empty(()) => None,
    };

    Ok(ChannelInconsistentReport {
        local_reset_terms_balance: read_custom_int128(
            &channel_inconsistent_report_reader.get_local_reset_terms_balance()?,
        )?,
        opt_remote_reset_terms,
    })
}

fn ser_channel_status_report(
    channel_status_report: &ChannelStatusReport,
    channel_status_report_builder: &mut report_capnp::channel_status_report::Builder,
) {
    match channel_status_report {
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            let mut inconsistent_builder =
                channel_status_report_builder.reborrow().init_inconsistent();
            ser_channel_inconsistent_report(channel_inconsistent_report, &mut inconsistent_builder);
        }
        ChannelStatusReport::Consistent(tc_report) => {
            let mut consistent_builder = channel_status_report_builder.reborrow().init_consistent();
            ser_tc_report(tc_report, &mut consistent_builder);
        }
    };
}

fn deser_channel_status_report(
    channel_status_report_reader: &report_capnp::channel_status_report::Reader,
) -> Result<ChannelStatusReport, SerializeError> {
    Ok(match channel_status_report_reader.which()? {
        report_capnp::channel_status_report::Inconsistent(channel_inconsistent_report_reader) => {
            ChannelStatusReport::Inconsistent(deser_channel_inconsistent_report(
                &channel_inconsistent_report_reader?,
            )?)
        }
        report_capnp::channel_status_report::Consistent(tc_report_reader) => {
            ChannelStatusReport::Consistent(deser_tc_report(&tc_report_reader?)?)
        }
    })
}

fn ser_opt_last_incoming_move_token(
    opt_last_incoming_move_token: &Option<MoveTokenHashedReport>,
    opt_last_incoming_move_token_builder: &mut report_capnp::opt_last_incoming_move_token::Builder,
) {
    match opt_last_incoming_move_token {
        Some(last_incoming_move_token) => {
            let mut move_token_hashed_builder = opt_last_incoming_move_token_builder
                .reborrow()
                .init_move_token_hashed();

            ser_move_token_hashed_report(last_incoming_move_token, &mut move_token_hashed_builder);
        }
        None => {
            opt_last_incoming_move_token_builder.set_empty(());
        }
    };
}

fn deser_opt_last_incoming_move_token(
    opt_last_incoming_move_token_reader: &report_capnp::opt_last_incoming_move_token::Reader,
) -> Result<Option<MoveTokenHashedReport>, SerializeError> {
    Ok(match opt_last_incoming_move_token_reader.which()? {
        report_capnp::opt_last_incoming_move_token::MoveTokenHashed(move_token_hashed_reader) => {
            Some(deser_move_token_hashed_report(&move_token_hashed_reader?)?)
        }
        report_capnp::opt_last_incoming_move_token::Empty(()) => None,
    })
}

fn ser_relays_transition(
    relays_transition: &(
        ImVec<NamedRelayAddress<NetAddress>>,
        ImVec<NamedRelayAddress<NetAddress>>,
    ),
    relays_transition_builder: &mut report_capnp::relays_transition::Builder,
) {
    let (last_sent, before_last_sent) = relays_transition;

    let mut last_sent_builder = relays_transition_builder
        .reborrow()
        .init_last_sent(usize_to_u32(last_sent.len()).unwrap());

    for (index, named_relay_address) in last_sent.iter().enumerate() {
        let mut named_relay_address_builder = last_sent_builder
            .reborrow()
            .get(usize_to_u32(index).unwrap());
        write_named_relay_address(named_relay_address, &mut named_relay_address_builder);
    }

    let mut before_last_sent_builder = relays_transition_builder
        .reborrow()
        .init_before_last_sent(usize_to_u32(before_last_sent.len()).unwrap());

    for (index, named_relay_address) in before_last_sent.iter().enumerate() {
        let mut named_relay_address_builder = before_last_sent_builder
            .reborrow()
            .get(usize_to_u32(index).unwrap());
        write_named_relay_address(named_relay_address, &mut named_relay_address_builder);
    }
}

type RelaysTransitions = (
    ImVec<NamedRelayAddress<NetAddress>>,
    ImVec<NamedRelayAddress<NetAddress>>,
);
fn deser_relays_transition(
    relays_transition_reader: &report_capnp::relays_transition::Reader,
) -> Result<RelaysTransitions, SerializeError> {
    let mut last_sent = ImVec::new();
    for named_relay_address in relays_transition_reader.get_last_sent()? {
        last_sent.push_back(read_named_relay_address(&named_relay_address)?);
    }

    let mut before_last_sent = ImVec::new();
    for named_relay_address in relays_transition_reader.get_before_last_sent()? {
        before_last_sent.push_back(read_named_relay_address(&named_relay_address)?);
    }

    Ok((last_sent, before_last_sent))
}

fn ser_sent_local_relays_report(
    sent_local_relays_report: &SentLocalRelaysReport,
    sent_local_relays_report_builder: &mut report_capnp::sent_local_relays_report::Builder,
) {
    match sent_local_relays_report {
        SentLocalRelaysReport::NeverSent => {
            sent_local_relays_report_builder.set_never_sent(());
        }
        SentLocalRelaysReport::Transition(relays_transition) => {
            let mut relays_transition_builder = sent_local_relays_report_builder
                .reborrow()
                .init_transition();
            ser_relays_transition(&relays_transition, &mut relays_transition_builder);
        }
        SentLocalRelaysReport::LastSent(last_sent) => {
            let relays_len = usize_to_u32(last_sent.len()).unwrap();
            let mut last_sent_builder = sent_local_relays_report_builder
                .reborrow()
                .init_last_sent(relays_len);
            for (index, named_relay_address) in last_sent.iter().enumerate() {
                let mut named_relay_address_builder = last_sent_builder
                    .reborrow()
                    .get(usize_to_u32(index).unwrap());
                write_named_relay_address(named_relay_address, &mut named_relay_address_builder);
            }
        }
    }
}

fn deser_sent_local_relays_report(
    sent_local_relays_report_reader: &report_capnp::sent_local_relays_report::Reader,
) -> Result<SentLocalRelaysReport, SerializeError> {
    Ok(match sent_local_relays_report_reader.which()? {
        report_capnp::sent_local_relays_report::NeverSent(()) => SentLocalRelaysReport::NeverSent,
        report_capnp::sent_local_relays_report::Transition(relays_transition_reader) => {
            SentLocalRelaysReport::Transition(deser_relays_transition(&relays_transition_reader?)?)
        }
        report_capnp::sent_local_relays_report::LastSent(last_sent_reader) => {
            let mut last_sent = Vec::new();
            for named_relay_address in last_sent_reader? {
                last_sent.push(read_named_relay_address(&named_relay_address)?);
            }
            SentLocalRelaysReport::LastSent(last_sent.into_iter().collect())
        }
    })
}

fn ser_friend_report(
    friend_report: &FriendReport,
    friend_report_builder: &mut report_capnp::friend_report::Builder,
) {
    friend_report_builder
        .reborrow()
        .set_name(&friend_report.name);

    write_rate(
        &friend_report.rate,
        &mut friend_report_builder.reborrow().init_rate(),
    );

    // remote_relays:
    let relays_len = usize_to_u32(friend_report.remote_relays.len()).unwrap();
    let mut relays_builder = friend_report_builder
        .reborrow()
        .init_remote_relays(relays_len);
    for (index, relay_address) in friend_report.remote_relays.iter().enumerate() {
        let mut relay_address_builder = relays_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_relay_address(relay_address, &mut relay_address_builder);
    }

    ser_sent_local_relays_report(
        &friend_report.sent_local_relays,
        &mut friend_report_builder.reborrow().init_sent_local_relays(),
    );

    ser_opt_last_incoming_move_token(
        &friend_report.opt_last_incoming_move_token,
        &mut friend_report_builder
            .reborrow()
            .init_opt_last_incoming_move_token(),
    );

    ser_friend_liveness_report(
        &friend_report.liveness,
        &mut friend_report_builder.reborrow().init_liveness(),
    );

    ser_channel_status_report(
        &friend_report.channel_status,
        &mut friend_report_builder.reborrow().init_channel_status(),
    );

    write_custom_u_int128(
        friend_report.wanted_remote_max_debt,
        &mut friend_report_builder
            .reborrow()
            .init_wanted_remote_max_debt(),
    );

    ser_requests_status_report(
        &friend_report.wanted_local_requests_status,
        &mut friend_report_builder
            .reborrow()
            .init_wanted_local_requests_status(),
    );

    friend_report_builder.set_num_pending_requests(friend_report.num_pending_requests);
    friend_report_builder.set_num_pending_backwards_ops(friend_report.num_pending_backwards_ops);

    ser_friend_status_report(
        &friend_report.status,
        &mut friend_report_builder.reborrow().init_status(),
    );

    friend_report_builder.set_num_pending_user_requests(friend_report.num_pending_user_requests);
}

fn deser_friend_report(
    friend_report_reader: &report_capnp::friend_report::Reader,
) -> Result<FriendReport, SerializeError> {
    let mut remote_relays = Vec::new();
    for relay_address in friend_report_reader.get_remote_relays()? {
        remote_relays.push(read_relay_address(&relay_address)?);
    }

    Ok(FriendReport {
        name: friend_report_reader.get_name()?.to_owned(),
        rate: read_rate(&friend_report_reader.get_rate()?)?,
        remote_relays,
        sent_local_relays: deser_sent_local_relays_report(
            &friend_report_reader.get_sent_local_relays()?,
        )?,
        opt_last_incoming_move_token: deser_opt_last_incoming_move_token(
            &friend_report_reader.get_opt_last_incoming_move_token()?,
        )?,
        liveness: deser_friend_liveness_report(&friend_report_reader.get_liveness()?)?,
        channel_status: deser_channel_status_report(&friend_report_reader.get_channel_status()?)?,
        wanted_remote_max_debt: read_custom_u_int128(
            &friend_report_reader.get_wanted_remote_max_debt()?,
        )?,
        wanted_local_requests_status: deser_requests_status_report(
            &friend_report_reader.get_wanted_local_requests_status()?,
        )?,
        num_pending_requests: friend_report_reader.get_num_pending_requests(),
        num_pending_backwards_ops: friend_report_reader.get_num_pending_backwards_ops(),
        status: deser_friend_status_report(&friend_report_reader.get_status()?)?,
        num_pending_user_requests: friend_report_reader.get_num_pending_user_requests(),
    })
}

fn ser_pk_friend_report(
    pk_friend_report: &(PublicKey, FriendReport),
    pk_friend_report_builder: &mut report_capnp::pk_friend_report::Builder,
) {
    let (friend_public_key, friend_report) = pk_friend_report;
    write_public_key(
        friend_public_key,
        &mut pk_friend_report_builder.reborrow().init_friend_public_key(),
    );
    ser_friend_report(
        friend_report,
        &mut pk_friend_report_builder.reborrow().init_friend_report(),
    );
}

fn deser_pk_friend_report(
    pk_friend_report_reader: &report_capnp::pk_friend_report::Reader,
) -> Result<(PublicKey, FriendReport), SerializeError> {
    let friend_public_key = read_public_key(&pk_friend_report_reader.get_friend_public_key()?)?;
    let friend_report = deser_friend_report(&pk_friend_report_reader.get_friend_report()?)?;

    Ok((friend_public_key, friend_report))
}

fn ser_funder_report(
    funder_report: &FunderReport,
    funder_report_builder: &mut report_capnp::funder_report::Builder,
) {
    write_public_key(
        &funder_report.local_public_key,
        &mut funder_report_builder.reborrow().init_local_public_key(),
    );

    let relays_len = usize_to_u32(funder_report.relays.len()).unwrap();
    let mut relays_builder = funder_report_builder.reborrow().init_relays(relays_len);
    for (index, named_relay_address) in funder_report.relays.iter().enumerate() {
        let mut named_relay_address_builder =
            relays_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_named_relay_address(named_relay_address, &mut named_relay_address_builder);
    }

    let friends_len = usize_to_u32(funder_report.friends.len()).unwrap();
    let mut friends_builder = funder_report_builder.reborrow().init_friends(friends_len);
    for (index, pk_friend) in funder_report.friends.iter().enumerate() {
        let mut pk_friend_builder = friends_builder.reborrow().get(usize_to_u32(index).unwrap());
        ser_pk_friend_report(pk_friend, &mut pk_friend_builder);
    }

    funder_report_builder.set_num_open_invoices(funder_report.num_open_invoices);
    funder_report_builder.set_num_payments(funder_report.num_payments);
    funder_report_builder.set_num_open_transactions(funder_report.num_open_transactions);
}

fn deser_funder_report(
    funder_report_reader: &report_capnp::funder_report::Reader,
) -> Result<FunderReport, SerializeError> {
    let mut named_relays = Vec::new();
    for named_relay_address in funder_report_reader.get_relays()? {
        named_relays.push(read_named_relay_address(&named_relay_address)?);
    }

    let mut friends = ImHashMap::new();
    for pk_friend in funder_report_reader.get_friends()? {
        let (friend_public_key, friend_report) = deser_pk_friend_report(&pk_friend)?;
        friends.insert(friend_public_key, friend_report);
    }

    Ok(FunderReport {
        local_public_key: read_public_key(&funder_report_reader.get_local_public_key()?)?,
        relays: named_relays.into_iter().collect(),
        friends,
        num_open_invoices: funder_report_reader.get_num_open_invoices(),
        num_payments: funder_report_reader.get_num_payments(),
        num_open_transactions: funder_report_reader.get_num_open_transactions(),
    })
}

fn ser_add_friend_report(
    add_friend_report: &AddFriendReport,
    add_friend_report_builder: &mut report_capnp::add_friend_report::Builder,
) {
    write_public_key(
        &add_friend_report.friend_public_key,
        &mut add_friend_report_builder
            .reborrow()
            .init_friend_public_key(),
    );

    add_friend_report_builder.set_name(&add_friend_report.name);

    let relays_len = usize_to_u32(add_friend_report.relays.len()).unwrap();
    let mut relays_builder = add_friend_report_builder.reborrow().init_relays(relays_len);
    for (index, relay_address) in add_friend_report.relays.iter().enumerate() {
        let mut relay_address_builder = relays_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_relay_address(relay_address, &mut relay_address_builder);
    }

    write_custom_int128(
        add_friend_report.balance,
        &mut add_friend_report_builder.reborrow().init_balance(),
    );

    ser_opt_last_incoming_move_token(
        &add_friend_report.opt_last_incoming_move_token,
        &mut add_friend_report_builder
            .reborrow()
            .init_opt_last_incoming_move_token(),
    );

    ser_channel_status_report(
        &add_friend_report.channel_status,
        &mut add_friend_report_builder.reborrow().init_channel_status(),
    );
}

fn deser_add_friend_report(
    add_friend_report_reader: &report_capnp::add_friend_report::Reader,
) -> Result<AddFriendReport, SerializeError> {
    let mut relays = Vec::new();
    for relay_address in add_friend_report_reader.get_relays()? {
        relays.push(read_relay_address(&relay_address)?);
    }

    Ok(AddFriendReport {
        friend_public_key: read_public_key(&add_friend_report_reader.get_friend_public_key()?)?,
        name: add_friend_report_reader.get_name()?.to_owned(),
        relays,
        balance: read_custom_int128(&add_friend_report_reader.get_balance()?)?,
        opt_last_incoming_move_token: deser_opt_last_incoming_move_token(
            &add_friend_report_reader.get_opt_last_incoming_move_token()?,
        )?,
        channel_status: deser_channel_status_report(
            &add_friend_report_reader.get_channel_status()?,
        )?,
    })
}

fn ser_friend_report_mutation(
    friend_report_mutation: &FriendReportMutation,
    friend_report_mutation_builder: &mut report_capnp::friend_report_mutation::Builder,
) {
    match friend_report_mutation {
        FriendReportMutation::SetRemoteRelays(relays) => {
            let relays_len = usize_to_u32(relays.len()).unwrap();
            let mut relays_builder = friend_report_mutation_builder
                .reborrow()
                .init_set_remote_relays(relays_len);
            for (index, relay_address) in relays.iter().enumerate() {
                let mut relay_address_builder =
                    relays_builder.reborrow().get(usize_to_u32(index).unwrap());
                write_relay_address(relay_address, &mut relay_address_builder);
            }
        }
        FriendReportMutation::SetName(name) => {
            friend_report_mutation_builder.reborrow().set_set_name(name)
        }
        FriendReportMutation::SetRate(rate) => write_rate(
            rate,
            &mut friend_report_mutation_builder.reborrow().init_set_rate(),
        ),
        FriendReportMutation::SetSentLocalRelays(sent_local_relays_report) => {
            ser_sent_local_relays_report(
                sent_local_relays_report,
                &mut friend_report_mutation_builder
                    .reborrow()
                    .init_set_sent_local_relays(),
            )
        }
        FriendReportMutation::SetChannelStatus(channel_status_report) => ser_channel_status_report(
            channel_status_report,
            &mut friend_report_mutation_builder
                .reborrow()
                .init_set_channel_status(),
        ),
        FriendReportMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
            write_custom_u_int128(
                *wanted_remote_max_debt,
                &mut friend_report_mutation_builder
                    .reborrow()
                    .init_set_wanted_remote_max_debt(),
            )
        }
        FriendReportMutation::SetWantedLocalRequestsStatus(requests_status_report) => {
            ser_requests_status_report(
                requests_status_report,
                &mut friend_report_mutation_builder
                    .reborrow()
                    .init_set_wanted_local_requests_status(),
            )
        }
        FriendReportMutation::SetNumPendingRequests(num_pending_requests) => {
            friend_report_mutation_builder
                .reborrow()
                .set_set_num_pending_requests(*num_pending_requests)
        }
        FriendReportMutation::SetNumPendingBackwardsOps(num_pending_backwards_ops) => {
            friend_report_mutation_builder
                .reborrow()
                .set_set_num_pending_backwards_ops(*num_pending_backwards_ops)
        }
        FriendReportMutation::SetStatus(friend_status_report) => ser_friend_status_report(
            friend_status_report,
            &mut friend_report_mutation_builder.reborrow().init_set_status(),
        ),
        FriendReportMutation::SetNumPendingUserRequests(num_pending_user_requests) => {
            friend_report_mutation_builder
                .reborrow()
                .set_set_num_pending_user_requests(*num_pending_user_requests)
        }
        FriendReportMutation::SetOptLastIncomingMoveToken(opt_last_incoming_move_token) => {
            ser_opt_last_incoming_move_token(
                opt_last_incoming_move_token,
                &mut friend_report_mutation_builder
                    .reborrow()
                    .init_set_opt_last_incoming_move_token(),
            )
        }
        FriendReportMutation::SetLiveness(friend_liveness_report) => ser_friend_liveness_report(
            friend_liveness_report,
            &mut friend_report_mutation_builder
                .reborrow()
                .init_set_liveness(),
        ),
    };
}

fn deser_friend_report_mutation(
    friend_report_mutation: &report_capnp::friend_report_mutation::Reader,
) -> Result<FriendReportMutation, SerializeError> {
    Ok(match friend_report_mutation.which()? {
        report_capnp::friend_report_mutation::SetRemoteRelays(relays_reader) => {
            let mut relays = Vec::new();
            for relay_address in relays_reader? {
                relays.push(read_relay_address(&relay_address)?);
            }
            FriendReportMutation::SetRemoteRelays(relays)
        }
        report_capnp::friend_report_mutation::SetName(name) => {
            FriendReportMutation::SetName(name?.to_owned())
        }
        report_capnp::friend_report_mutation::SetRate(rate_reader) => {
            FriendReportMutation::SetRate(read_rate(&rate_reader?)?)
        }
        report_capnp::friend_report_mutation::SetSentLocalRelays(
            sent_local_relays_report_reader,
        ) => FriendReportMutation::SetSentLocalRelays(deser_sent_local_relays_report(
            &sent_local_relays_report_reader?,
        )?),
        report_capnp::friend_report_mutation::SetChannelStatus(channel_status_report_reader) => {
            FriendReportMutation::SetChannelStatus(deser_channel_status_report(
                &channel_status_report_reader?,
            )?)
        }
        report_capnp::friend_report_mutation::SetWantedRemoteMaxDebt(
            wanted_remote_max_debt_reader,
        ) => FriendReportMutation::SetWantedRemoteMaxDebt(read_custom_u_int128(
            &wanted_remote_max_debt_reader?,
        )?),
        report_capnp::friend_report_mutation::SetWantedLocalRequestsStatus(
            wanted_local_requests_status_reader,
        ) => FriendReportMutation::SetWantedLocalRequestsStatus(deser_requests_status_report(
            &wanted_local_requests_status_reader?,
        )?),
        report_capnp::friend_report_mutation::SetNumPendingRequests(num_pending_requests) => {
            FriendReportMutation::SetNumPendingRequests(num_pending_requests)
        }
        report_capnp::friend_report_mutation::SetNumPendingBackwardsOps(
            num_pending_backwards_ops,
        ) => FriendReportMutation::SetNumPendingBackwardsOps(num_pending_backwards_ops),
        report_capnp::friend_report_mutation::SetStatus(friend_status_report_reader) => {
            FriendReportMutation::SetStatus(deser_friend_status_report(
                &friend_status_report_reader?,
            )?)
        }
        report_capnp::friend_report_mutation::SetNumPendingUserRequests(
            num_pending_user_requests,
        ) => FriendReportMutation::SetNumPendingUserRequests(num_pending_user_requests),
        report_capnp::friend_report_mutation::SetOptLastIncomingMoveToken(
            opt_last_incoming_move_token_reader,
        ) => FriendReportMutation::SetOptLastIncomingMoveToken(deser_opt_last_incoming_move_token(
            &opt_last_incoming_move_token_reader?,
        )?),
        report_capnp::friend_report_mutation::SetLiveness(friend_liveness_report_reader) => {
            FriendReportMutation::SetLiveness(deser_friend_liveness_report(
                &friend_liveness_report_reader?,
            )?)
        }
    })
}

fn ser_pk_friend_report_mutation(
    pk_friend_report_mutation: &(PublicKey, FriendReportMutation),
    pk_friend_report_mutation_builder: &mut report_capnp::pk_friend_report_mutation::Builder,
) {
    let (friend_public_key, friend_report_mutation) = pk_friend_report_mutation;
    write_public_key(
        &friend_public_key,
        &mut pk_friend_report_mutation_builder
            .reborrow()
            .init_friend_public_key(),
    );
    ser_friend_report_mutation(
        &friend_report_mutation,
        &mut pk_friend_report_mutation_builder
            .reborrow()
            .init_friend_report_mutation(),
    );
}

fn deser_pk_friend_report_mutation(
    pk_friend_report_mutation_reader: &report_capnp::pk_friend_report_mutation::Reader,
) -> Result<(PublicKey, FriendReportMutation), SerializeError> {
    let friend_public_key =
        read_public_key(&pk_friend_report_mutation_reader.get_friend_public_key()?)?;
    let friend_report_mutation = deser_friend_report_mutation(
        &pk_friend_report_mutation_reader.get_friend_report_mutation()?,
    )?;

    Ok((friend_public_key, friend_report_mutation))
}

fn ser_funder_report_mutation(
    funder_report_mutation: &FunderReportMutation,
    funder_report_mutation_builder: &mut report_capnp::funder_report_mutation::Builder,
) {
    match funder_report_mutation {
        FunderReportMutation::AddRelay(named_relay_address) => {
            write_named_relay_address(
                named_relay_address,
                &mut funder_report_mutation_builder.reborrow().init_add_relay(),
            );
        }
        FunderReportMutation::RemoveRelay(public_key) => {
            write_public_key(
                public_key,
                &mut funder_report_mutation_builder
                    .reborrow()
                    .init_remove_relay(),
            );
        }
        FunderReportMutation::AddFriend(add_friend_report) => {
            ser_add_friend_report(
                add_friend_report,
                &mut funder_report_mutation_builder.reborrow().init_add_friend(),
            );
        }
        FunderReportMutation::RemoveFriend(friend_public_key) => {
            write_public_key(
                friend_public_key,
                &mut funder_report_mutation_builder
                    .reborrow()
                    .init_remove_friend(),
            );
        }
        FunderReportMutation::FriendReportMutation(pk_friend_report_mutation) => {
            ser_pk_friend_report_mutation(
                pk_friend_report_mutation,
                &mut funder_report_mutation_builder
                    .reborrow()
                    .init_pk_friend_report_mutation(),
            );
        }
        FunderReportMutation::SetNumOpenInvoices(num_open_invoices) => {
            funder_report_mutation_builder
                .reborrow()
                .set_set_num_open_invoices(*num_open_invoices);
        }
        FunderReportMutation::SetNumPayments(num_payments) => {
            funder_report_mutation_builder
                .reborrow()
                .set_set_num_payments(*num_payments);
        }
        FunderReportMutation::SetNumOpenTransactions(num_open_transactions) => {
            funder_report_mutation_builder
                .reborrow()
                .set_set_num_open_transactions(*num_open_transactions);
        }
    }
}

fn deser_funder_report_mutation(
    funder_report_mutation_reader: &report_capnp::funder_report_mutation::Reader,
) -> Result<FunderReportMutation, SerializeError> {
    Ok(match funder_report_mutation_reader.which()? {
        report_capnp::funder_report_mutation::AddRelay(named_relay_address_reader) => {
            FunderReportMutation::AddRelay(read_named_relay_address(&named_relay_address_reader?)?)
        }
        report_capnp::funder_report_mutation::RemoveRelay(public_key_reader) => {
            FunderReportMutation::RemoveRelay(read_public_key(&public_key_reader?)?)
        }
        report_capnp::funder_report_mutation::AddFriend(add_friend_report_reader) => {
            FunderReportMutation::AddFriend(deser_add_friend_report(&add_friend_report_reader?)?)
        }
        report_capnp::funder_report_mutation::RemoveFriend(friend_public_key_reader) => {
            FunderReportMutation::RemoveFriend(read_public_key(&friend_public_key_reader?)?)
        }
        report_capnp::funder_report_mutation::PkFriendReportMutation(
            pk_friend_report_mutation_reader,
        ) => FunderReportMutation::FriendReportMutation(deser_pk_friend_report_mutation(
            &pk_friend_report_mutation_reader?,
        )?),
        report_capnp::funder_report_mutation::SetNumOpenInvoices(num_open_invoices) => {
            FunderReportMutation::SetNumOpenInvoices(num_open_invoices)
        }
        report_capnp::funder_report_mutation::SetNumPayments(num_payments) => {
            FunderReportMutation::SetNumPayments(num_payments)
        }
        report_capnp::funder_report_mutation::SetNumOpenTransactions(set_num_open_transactions) => {
            FunderReportMutation::SetNumOpenTransactions(set_num_open_transactions)
        }
    })
}

fn ser_index_client_report(
    index_client_report: &IndexClientReport<NetAddress>,
    index_client_report_builder: &mut report_capnp::index_client_report::Builder,
) {
    let index_servers_len = usize_to_u32(index_client_report.index_servers.len()).unwrap();
    let mut index_servers_builder = index_client_report_builder
        .reborrow()
        .init_index_servers(index_servers_len);
    for (index, named_index_server) in index_client_report.index_servers.iter().enumerate() {
        let mut named_index_server_builder = index_servers_builder
            .reborrow()
            .get(usize_to_u32(index).unwrap());
        write_named_index_server_address(named_index_server, &mut named_index_server_builder);
    }

    let mut opt_connected_server_builder = index_client_report_builder
        .reborrow()
        .init_opt_connected_server();
    match &index_client_report.opt_connected_server {
        Some(public_key) => {
            write_public_key(
                public_key,
                &mut opt_connected_server_builder.init_public_key(),
            );
        }
        None => {
            opt_connected_server_builder.set_empty(());
        }
    }
}

fn deser_index_client_report(
    index_client_report_reader: &report_capnp::index_client_report::Reader,
) -> Result<IndexClientReport<NetAddress>, SerializeError> {
    let mut index_servers = Vec::new();
    for named_index_server_reader in index_client_report_reader.get_index_servers()? {
        index_servers.push(read_named_index_server_address(&named_index_server_reader)?);
    }

    let opt_connected_server = match index_client_report_reader
        .get_opt_connected_server()
        .which()?
    {
        report_capnp::index_client_report::opt_connected_server::PublicKey(public_key_reader) => {
            Some(read_public_key(&public_key_reader?)?)
        }
        report_capnp::index_client_report::opt_connected_server::Empty(()) => None,
    };

    Ok(IndexClientReport {
        index_servers,
        opt_connected_server,
    })
}

fn ser_index_client_report_mutation(
    index_client_report_mutation: &IndexClientReportMutation<NetAddress>,
    index_client_report_mutation_builder: &mut report_capnp::index_client_report_mutation::Builder,
) {
    match index_client_report_mutation {
        IndexClientReportMutation::AddIndexServer(named_index_server_address) => {
            write_named_index_server_address(
                named_index_server_address,
                &mut index_client_report_mutation_builder
                    .reborrow()
                    .init_add_index_server(),
            )
        }
        IndexClientReportMutation::RemoveIndexServer(public_key) => write_public_key(
            public_key,
            &mut index_client_report_mutation_builder
                .reborrow()
                .init_remove_index_server(),
        ),
        IndexClientReportMutation::SetConnectedServer(opt_index_server_address) => {
            let mut set_connected_server_builder = index_client_report_mutation_builder
                .reborrow()
                .init_set_connected_server();
            match opt_index_server_address {
                Some(public_key) => write_public_key(
                    public_key,
                    &mut set_connected_server_builder.init_public_key(),
                ),
                None => set_connected_server_builder.set_empty(()),
            }
        }
    }
}

fn deser_index_client_report_mutation(
    index_client_report_mutation_reader: &report_capnp::index_client_report_mutation::Reader,
) -> Result<IndexClientReportMutation<NetAddress>, SerializeError> {
    Ok(match index_client_report_mutation_reader.which()? {
        report_capnp::index_client_report_mutation::AddIndexServer(
            named_index_server_address_reader,
        ) => IndexClientReportMutation::AddIndexServer(read_named_index_server_address(
            &named_index_server_address_reader?,
        )?),
        report_capnp::index_client_report_mutation::RemoveIndexServer(public_key_reader) => {
            IndexClientReportMutation::RemoveIndexServer(read_public_key(&public_key_reader?)?)
        }
        report_capnp::index_client_report_mutation::SetConnectedServer(
            set_connected_server_reader,
        ) => match set_connected_server_reader.which()? {
            report_capnp::index_client_report_mutation::set_connected_server::PublicKey(
                public_key_reader,
            ) => IndexClientReportMutation::SetConnectedServer(Some(read_public_key(
                &public_key_reader?,
            )?)),
            report_capnp::index_client_report_mutation::set_connected_server::Empty(()) => {
                IndexClientReportMutation::SetConnectedServer(None)
            }
        },
    })
}

pub fn ser_node_report(
    node_report: &NodeReport,
    node_report_builder: &mut report_capnp::node_report::Builder,
) {
    ser_funder_report(
        &node_report.funder_report,
        &mut node_report_builder.reborrow().init_funder_report(),
    );
    ser_index_client_report(
        &node_report.index_client_report,
        &mut node_report_builder.reborrow().init_index_client_report(),
    );
}

pub fn deser_node_report(
    node_report_reader: &report_capnp::node_report::Reader,
) -> Result<NodeReport, SerializeError> {
    Ok(NodeReport {
        funder_report: deser_funder_report(&node_report_reader.get_funder_report()?)?,
        index_client_report: deser_index_client_report(
            &node_report_reader.get_index_client_report()?,
        )?,
    })
}

pub fn ser_node_report_mutation(
    node_report_mutation: &NodeReportMutation,
    node_report_mutation_builder: &mut report_capnp::node_report_mutation::Builder,
) {
    match node_report_mutation {
        NodeReportMutation::Funder(funder_report_mutation) => ser_funder_report_mutation(
            &funder_report_mutation,
            &mut node_report_mutation_builder.reborrow().init_funder(),
        ),
        NodeReportMutation::IndexClient(index_client_report_mutation) => {
            ser_index_client_report_mutation(
                &index_client_report_mutation,
                &mut node_report_mutation_builder.reborrow().init_index_client(),
            )
        }
    }
}

pub fn deser_node_report_mutation(
    node_report_mutation_reader: &report_capnp::node_report_mutation::Reader,
) -> Result<NodeReportMutation, SerializeError> {
    Ok(match node_report_mutation_reader.which()? {
        report_capnp::node_report_mutation::Funder(funder_report_mutation_reader) => {
            NodeReportMutation::Funder(deser_funder_report_mutation(
                &funder_report_mutation_reader?,
            )?)
        }
        report_capnp::node_report_mutation::IndexClient(index_client_report_mutation_reader) => {
            NodeReportMutation::IndexClient(deser_index_client_report_mutation(
                &index_client_report_mutation_reader?,
            )?)
        }
    })
}
