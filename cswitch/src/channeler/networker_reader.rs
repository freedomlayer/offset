
struct NetworkerReader;

/*
fn handle_networker_message(&mut self, message: NetworkerToChanneler) ->
    StartSend<NetworkerToChanneler, ChannelerError> {

    match message {
        NetworkerToChanneler::SendChannelMessage { 
            neighbor_public_key, message_content } => {
            // TODO: Attempt to send a message to some TCP connections leading to
            // the requested neighbor. The chosen Sink could be not ready.
            Ok(AsyncSink::Ready)
        },
        NetworkerToChanneler::AddNeighborRelation { neighbor_info } => {
            let neighbor_public_key = neighbor_info.neighbor_public_key.clone();
            if self.neighbors.contains_key(&neighbor_public_key) {
                warn!("Neighbor with public key {:?} already exists", 
                      neighbor_public_key);
            }
            self.neighbors.insert(neighbor_public_key, ChannelerNeighbor {
                info: neighbor_info,
                last_remote_rand_value: None,
                channels: Vec::new(),
                pending_channels: Vec::new(),
                pending_out_conn: None,
                ticks_to_next_conn_attempt: CONN_ATTEMPT_TICKS,
            });
            Ok(AsyncSink::Ready)
        },
        NetworkerToChanneler::RemoveNeighborRelation { neighbor_public_key } => {
            match self.neighbors.remove(&neighbor_public_key) {
                None => warn!("Attempt to remove a nonexistent neighbor \
                    relation with public key {:?}", neighbor_public_key),
                _ => {},
            };
            // TODO: Possibly close all connections here.

            Ok(AsyncSink::Ready)
        },
        NetworkerToChanneler::SetMaxChannels 
            { neighbor_public_key, max_channels } => {
            match self.neighbors.get_mut(&neighbor_public_key) {
                None => warn!("Attempt to change max_channels for a \
                    nonexistent neighbor relation with public key {:?}",
                    neighbor_public_key),
                Some(neighbor) => {
                    neighbor.info.max_channels = max_channels;
                },
            };
            Ok(AsyncSink::Ready)
        },
        NetworkerToChanneler::SetServerType(server_type) => {
            self.server_type = server_type;
            Ok(AsyncSink::Ready)
        },
    }
}
*/
