use common::int_convert::usize_to_u32;
use crate::capnp_common::{write_signature, read_signature,
                          write_custom_int128, read_custom_int128,
                          write_custom_u_int128, read_custom_u_int128,
                          write_rand_nonce, read_rand_nonce,
                          write_uid, read_uid,
                          write_invoice_id, read_invoice_id,
                          write_public_key, read_public_key,
                          write_relay_address, read_relay_address,
                          write_hash, read_hash};
use index_capnp;

use super::messages::{RequestRoutes, RouteWithCapacity, ResponseRoutes,
                        UpdateFriend, IndexMutation, MutationsUpdate, TimeProofLink,
                        ForwardMutationsUpdate, IndexServerToClient, 
                        IndexClientToServer, IndexServerToServer};

use crate::funder::serialize::{ser_friends_route, deser_friends_route};

use crate::serialize::SerializeError;

fn ser_request_routes(request_routes: &RequestRoutes,
                         request_routes_builder: &mut index_capnp::request_routes::Builder) {

    write_uid(&request_routes.request_id, &mut request_routes_builder.reborrow().init_request_id());
    write_custom_u_int128(request_routes.capacity, &mut request_routes_builder.reborrow().init_capacity());
    write_public_key(&request_routes.source, &mut request_routes_builder.reborrow().init_source());
    write_public_key(&request_routes.destination, &mut request_routes_builder.reborrow().init_destination());

    let mut opt_exclude_builder = request_routes_builder.reborrow().init_opt_exclude();
    match &request_routes.opt_exclude {
        Some((from_public_key, to_public_key)) => {
            let mut edge_builder = opt_exclude_builder.init_edge();
            write_public_key(from_public_key, &mut edge_builder.reborrow().init_from_public_key());
            write_public_key(to_public_key, &mut edge_builder.reborrow().init_to_public_key());
        },
        None => {
            opt_exclude_builder.set_empty(());
        },
    }
}

fn deser_request_routes(request_routes_reader: &index_capnp::request_routes::Reader)
    -> Result<RequestRoutes, SerializeError> {

    let opt_exclude = match request_routes_reader.get_opt_exclude().which()? {
        index_capnp::request_routes::opt_exclude::Edge(res_edge_reader) => {
            let edge_reader = res_edge_reader?;
            let from_public_key = read_public_key(&edge_reader.get_from_public_key()?)?;
            let to_public_key = read_public_key(&edge_reader.get_to_public_key()?)?;
            Some((from_public_key, to_public_key))
        },
        index_capnp::request_routes::opt_exclude::Empty(()) => None,
    };

    Ok(RequestRoutes {
        request_id: read_uid(&request_routes_reader.get_request_id()?)?,
        capacity: read_custom_u_int128(&request_routes_reader.get_capacity()?)?,
        source: read_public_key(&request_routes_reader.get_source()?)?,
        destination: read_public_key(&request_routes_reader.get_destination()?)?,
        opt_exclude,
    })
}


fn ser_route_with_capacity(route_with_capacity: &RouteWithCapacity,
                           route_with_capacity_builder: &mut index_capnp::route_with_capacity::Builder) {

    ser_friends_route(&route_with_capacity.route, &mut route_with_capacity_builder.reborrow().init_route());
    write_custom_u_int128(route_with_capacity.capacity, &mut route_with_capacity_builder.reborrow().init_capacity());
}

fn deser_route_with_capacity(route_with_capacity_reader: &index_capnp::route_with_capacity::Reader) 
    -> Result<RouteWithCapacity, SerializeError> {

    Ok(RouteWithCapacity {
        route: deser_friends_route(&route_with_capacity_reader.get_route()?)?,
        capacity: read_custom_u_int128(&route_with_capacity_reader.get_capacity()?)?,
    })
}

fn ser_response_routes(response_routes: &ResponseRoutes,
                       response_routes_builder: &mut index_capnp::response_routes::Builder) {

    write_uid(&response_routes.request_id, &mut response_routes_builder.reborrow().init_request_id());
    let routes_len = usize_to_u32(response_routes.routes.len()).unwrap();
    let mut routes_builder = response_routes_builder.reborrow().init_routes(routes_len);

    for (index, route) in response_routes.routes.iter().enumerate() {
        let mut route_with_capacity_builder = routes_builder.reborrow().get(usize_to_u32(index).unwrap());
        ser_route_with_capacity(&route, &mut route_with_capacity_builder);
    }
}

fn deser_response_routes(response_routes_reader: &index_capnp::response_routes::Reader)
    -> Result<ResponseRoutes, SerializeError> {
    
    let mut routes = Vec::new();
    for route_with_capacity in response_routes_reader.get_routes()? {
        routes.push(deser_route_with_capacity(&route_with_capacity)?);
    }

    Ok(ResponseRoutes {
        request_id: read_uid(&response_routes_reader.get_request_id()?)?,
        routes,
    })
}

fn ser_update_friend(update_friend: &UpdateFriend,
                       update_friend_builder: &mut index_capnp::update_friend::Builder) {

    write_public_key(&update_friend.public_key, &mut update_friend_builder.reborrow().init_public_key());
    write_custom_u_int128(update_friend.send_capacity, &mut update_friend_builder.reborrow().init_send_capacity());
    write_custom_u_int128(update_friend.recv_capacity, &mut update_friend_builder.reborrow().init_recv_capacity());
}

fn deser_update_friend(update_friend_reader: &index_capnp::update_friend::Reader)
    -> Result<UpdateFriend, SerializeError> {

    Ok(UpdateFriend {
        public_key: read_public_key(&update_friend_reader.get_public_key()?)?,
        send_capacity: read_custom_u_int128(&update_friend_reader.get_send_capacity()?)?,
        recv_capacity: read_custom_u_int128(&update_friend_reader.get_recv_capacity()?)?,
    })
}

fn ser_index_mutation(index_mutation: &IndexMutation,
                       index_mutation_builder: &mut index_capnp::index_mutation::Builder) {

    match &index_mutation {
        IndexMutation::UpdateFriend(update_friend) => {
            let mut update_friend_builder = index_mutation_builder.reborrow().init_update_friend();
            ser_update_friend(update_friend, &mut update_friend_builder);
        },
        IndexMutation::RemoveFriend(public_key) => {
            let mut remove_friend_builder = index_mutation_builder.reborrow().init_remove_friend();
            write_public_key(public_key, &mut remove_friend_builder);
        },
    }
}

fn deser_index_mutation(index_mutation_reader: &index_capnp::index_mutation::Reader)
    -> Result<IndexMutation, SerializeError> {

    Ok(match index_mutation_reader.which()? {
        index_capnp::index_mutation::UpdateFriend(update_friend_reader_res) => {
            let update_friend_reader = update_friend_reader_res?;
            let update_friend = deser_update_friend(&update_friend_reader)?;
            IndexMutation::UpdateFriend(update_friend)
        },
        index_capnp::index_mutation::RemoveFriend(remove_friend_reader_res) => {
            let remove_friend_reader = remove_friend_reader_res?;
            let public_key = read_public_key(&remove_friend_reader)?;
            IndexMutation::RemoveFriend(public_key)
        },
    })
}


fn ser_mutations_update(mutations_update: &MutationsUpdate,
                       mutations_update_builder: &mut index_capnp::mutations_update::Builder) {

    write_public_key(&mutations_update.node_public_key, 
                     &mut mutations_update_builder.reborrow().init_node_public_key());

    let mutations_len = usize_to_u32(mutations_update.index_mutations.len()).unwrap();
    let mut mutations_builder = mutations_update_builder.reborrow().init_index_mutations(mutations_len);

    for (index, index_mutation) in mutations_update.index_mutations.iter().enumerate() {
        let mut index_mutation_builder = mutations_builder.reborrow().get(usize_to_u32(index).unwrap());
        ser_index_mutation(index_mutation, &mut index_mutation_builder);
    }

    write_hash(&mutations_update.time_hash, 
               &mut mutations_update_builder.reborrow().init_time_hash());
    write_uid(&mutations_update.session_id, 
              &mut mutations_update_builder.reborrow().init_session_id());
    mutations_update_builder.reborrow().set_counter(mutations_update.counter);
    write_rand_nonce(&mutations_update.rand_nonce, 
                     &mut mutations_update_builder.reborrow().init_rand_nonce());
    write_signature(&mutations_update.signature, 
                    &mut mutations_update_builder.reborrow().init_signature());
}

fn deser_mutations_update(mutations_update_reader: &index_capnp::mutations_update::Reader)
    -> Result<MutationsUpdate, SerializeError> {

    let mut index_mutations = Vec::new();
    for index_mutation_reader in mutations_update_reader.get_index_mutations()? {
        index_mutations.push(deser_index_mutation(&index_mutation_reader)?);
    }

    Ok(MutationsUpdate {
        node_public_key: read_public_key(&mutations_update_reader.get_node_public_key()?)?,
        index_mutations,
        time_hash: read_hash(&mutations_update_reader.get_time_hash()?)?,
        session_id: read_uid(&mutations_update_reader.get_session_id()?)?,
        counter: mutations_update_reader.get_counter(),
        rand_nonce: read_rand_nonce(&mutations_update_reader.get_rand_nonce()?)?,
        signature: read_signature(&mutations_update_reader.get_signature()?)?,
    })
}

fn ser_time_proof_link(time_proof_link: &TimeProofLink,
                       time_proof_link_builder: &mut index_capnp::time_proof_link::Builder) {

    let hashes_len = usize_to_u32(time_proof_link.hashes.len()).unwrap();
    let mut hashes_builder = time_proof_link_builder.reborrow().init_hashes(hashes_len);

    for (index, hash_result) in time_proof_link.hashes.iter().enumerate() {
        let mut hash_builder = hashes_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_hash(hash_result, &mut hash_builder);
    }
}


fn deser_time_proof_link(time_proof_link_reader: &index_capnp::time_proof_link::Reader)
    -> Result<TimeProofLink, SerializeError> {

    let mut hashes = Vec::new();
    for hash_reader in time_proof_link_reader.get_hashes()? {
        hashes.push(read_hash(&hash_reader)?);
    }

    Ok(TimeProofLink {
        hashes,
    })
}

fn ser_forward_mutations_update(forward_mutations_update: &ForwardMutationsUpdate,
                       forward_mutations_update_builder: &mut index_capnp::forward_mutations_update::Builder) {

    ser_mutations_update(&forward_mutations_update.mutations_update, 
                         &mut forward_mutations_update_builder.reborrow().init_mutations_update());

    let time_proof_chain_len = usize_to_u32(
        forward_mutations_update.time_proof_chain.len()).unwrap();
    let mut time_proof_chain_builder = forward_mutations_update_builder.reborrow().init_time_proof_chain(time_proof_chain_len);

    for (index, time_proof_link) in forward_mutations_update.time_proof_chain.iter().enumerate() {
        let mut time_proof_link_builder = time_proof_chain_builder.reborrow().get(usize_to_u32(index).unwrap());
        ser_time_proof_link(time_proof_link, &mut time_proof_link_builder);
    }
}

fn deser_forward_mutations_update(forward_mutations_update_reader: &index_capnp::forward_mutations_update::Reader)
    -> Result<ForwardMutationsUpdate, SerializeError> {

    let mut time_proof_chain = Vec::new();
    for time_proof_link_reader in forward_mutations_update_reader.get_time_proof_chain()? {
        time_proof_chain.push(deser_time_proof_link(&time_proof_link_reader)?);
    }

    Ok(ForwardMutationsUpdate {
        mutations_update: deser_mutations_update(
                              &forward_mutations_update_reader.get_mutations_update()?)?,
        time_proof_chain,
    })
}

fn ser_index_server_to_client(index_server_to_client: &IndexServerToClient,
                       index_server_to_client_builder: &mut index_capnp::index_server_to_client::Builder) {

    match index_server_to_client {
        IndexServerToClient::TimeHash(hash_result) => {
            let mut hash_builder = index_server_to_client_builder.reborrow().init_time_hash();
            write_hash(hash_result, &mut hash_builder);
        },
        IndexServerToClient::ResponseRoutes(response_routes) => {
            let mut response_routes_builder = index_server_to_client_builder.reborrow().init_response_routes();
            ser_response_routes(response_routes, &mut response_routes_builder);
        },
    }
}

fn deser_index_server_to_client(index_server_to_client_reader: &index_capnp::index_server_to_client::Reader)
    -> Result<IndexServerToClient, SerializeError> {

    Ok(match index_server_to_client_reader.which()? {
        index_capnp::index_server_to_client::TimeHash(hash_reader) => {
            IndexServerToClient::TimeHash(read_hash(&hash_reader?)?)
        },
        index_capnp::index_server_to_client::ResponseRoutes(response_routes_reader) => {
            IndexServerToClient::ResponseRoutes(deser_response_routes(&response_routes_reader?)?)
        },
    })
}


fn ser_index_client_to_server(index_client_to_server: &IndexClientToServer,
                       index_client_to_server_builder: &mut index_capnp::index_client_to_server::Builder) {

    match index_client_to_server {
        IndexClientToServer::MutationsUpdate(mutations_update) => {
            let mut mutations_update_builder = index_client_to_server_builder.reborrow().init_mutations_update();
            ser_mutations_update(mutations_update, &mut mutations_update_builder);
        },
        IndexClientToServer::RequestRoutes(request_routes) => {
            let mut request_routes_builder = index_client_to_server_builder.reborrow().init_request_routes();
            ser_request_routes(request_routes, &mut request_routes_builder);
        },
    }
}

fn deser_index_client_to_server(index_client_to_server_reader: &index_capnp::index_client_to_server::Reader)
    -> Result<IndexClientToServer, SerializeError> {

    Ok(match index_client_to_server_reader.which()? {
        index_capnp::index_client_to_server::MutationsUpdate(mutations_update_reader) => {
            IndexClientToServer::MutationsUpdate(deser_mutations_update(&mutations_update_reader?)?)
        },
        index_capnp::index_client_to_server::RequestRoutes(request_routes_reader) => {
            IndexClientToServer::RequestRoutes(deser_request_routes(&request_routes_reader?)?)
        },
    })
}


fn ser_index_server_to_server(index_server_to_server: &IndexServerToServer,
                       index_server_to_server_builder: &mut index_capnp::index_server_to_server::Builder) {

    match index_server_to_server {
        IndexServerToServer::TimeHash(hash_result) => {
            let mut hash_builder = index_server_to_server_builder.reborrow().init_time_hash();
            write_hash(hash_result, &mut hash_builder);
        },
        IndexServerToServer::ForwardMutationsUpdate(forward_mutations_update) => {
            let mut forward_mutations_update_builder = index_server_to_server_builder.reborrow().init_forward_mutations_update();
            ser_forward_mutations_update(forward_mutations_update, &mut forward_mutations_update_builder);
        },
    }
}

fn deser_index_server_to_server(index_server_to_server_reader: &index_capnp::index_server_to_server::Reader)
    -> Result<IndexServerToServer, SerializeError> {

    Ok(match index_server_to_server_reader.which()? {
        index_capnp::index_server_to_server::TimeHash(hash_reader) => {
            IndexServerToServer::TimeHash(read_hash(&hash_reader?)?)
        },
        index_capnp::index_server_to_server::ForwardMutationsUpdate(forward_mutations_update_reader) => {
            IndexServerToServer::ForwardMutationsUpdate(deser_forward_mutations_update(&forward_mutations_update_reader?)?)
        },
    })
}
