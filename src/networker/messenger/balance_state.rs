use std::collections::HashMap;
use super::token_channel::TokenChannel;
use crypto::identity::PublicKey;


pub struct BalanceState {
    token_channels: HashMap<PublicKey, Vec<TokenChannel>>,
    local_pk: PublicKey,

}


impl BalanceState{
    pub fn app_set_remote_max_debt(new_remote_max_debt: u64, remote_key: PublicKey){
    }

    pub fn app_remove_neighbor(remote_key: PublicKey){
    }

    pub fn atomic_process_trans_list(token_channel_index: u32,
                                     remote_public_key: &PublicKey,
                                     transactions: Vec<NetworkerTCTransaction>) {
    }
}


fn maximal_remote_pending(max_remote_debts: Vec<u64>, dest_max_remote_debt: u64,
                          src_max_remote_debt: u64) -> u64{
}
