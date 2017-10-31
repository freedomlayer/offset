

struct NetworkerStateMachine {

    // - Sqlite connection?
    // - outgoing messages queue

}

impl NetworkerStateMachine {
    // Interface with Timer:
    // --------------------
    fn timer_time_tick() {
    }

    // Interface with Database:
    // ------------------------
    
    // - add_neighbor
    // - remove_neighbor
    // - request_get_neighbors_list
    
    fn database_response_get_neighbors_list() {
    }

    fn database_update_token_channel_info() {
    }

    // - request_token_channel_state
    
    fn database_response_token_channel_info() {
    }

    // Interface with Security Module:
    // ------------------------------
    
    // - security_module_request_signature
    
    fn security_module_response_signature() {
    }

    // Interface with Plugin Manager:
    // -----------------------------
    
    fn plugin_manager_set_neighbor_capacity() {
    }

    fn plugin_manager_set_neighbor_max_token_channels() {
    }

    fn plugin_manager_add_neighbor() {
    }

    fn plugin_manager_remove_neighbor() {
    }

    fn plugin_manager_get_neighbors_list() {
    }

    fn plugin_manager_indexer_announce_self() {
    }

    // - message_received
    // - neighbors_update
    // - invalid_neighbor_move_token
    

    // Interface with Channeler:
    // ------------------------

    fn channeler_channel_opened() {
    }

    fn channeler_channel_closed() {
    }

    // - close_channel
    // - send_channel_message
    

    fn channeler_channel_message_received() {
    }

    // - request_add_neighbor_relation
    
    fn channeler_response_add_neighbor_relation() {
    }

    // - request_remove_neighbor_relation
    
    fn channeler_response_remove_neighbor_relation() {
    }

    // - request_neighbors_relation_list
    
    fn channeler_response_neighbors_relation_list() {
    }

    // - request_set_neighbor_max_token_channels
    
    fn channeler_response_set_neighbor_max_token_channels() {
    }

    // Interface with Indexer Client
    // -----------------------------

    // - indexer_announce
    // - message_received
    
    fn indexer_client_request_send_message() {
    }

    // - response_send_message

    // - notify_structure_change
    

    // Interface with Pather
    // ---------------------
    

    fn pather_request_send_message() {
    }

    // - response_send_message
    

    // Interface with Funder
    // ---------------------
    
    // - funder_request_send_funds
    
    fn funder_response_send_funds() {
    }

    // - message_received
    // - fund_received
    

    // Interface with remote Networker
    // -------------------------------
    fn request_token() {
    }

    fn move_token() {
    }

    // Output messages:
    // - invalid_move_token
    // - request_token
    // - move_token
}


