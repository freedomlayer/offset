use std::collections::HashMap;
use super::token_channel::TokenChannel;
use super::messenger_messages::NetworkerTCMessage;
use crypto::identity::PublicKey;


pub struct BalanceState {
    token_channels: HashMap<PublicKey, Vec<TokenChannel>>,
    local_pk: PublicKey,

}


impl BalanceState{
    pub fn app_set_remote_max_debt(&mut self, new_remote_max_debt: u64, remote_key: PublicKey){
    }

    pub fn app_remove_neighbor(&mut self, remote_key: PublicKey){
    }

    pub fn atomic_process_messagess_list(&mut self,
                                         token_channel_index: usize,
                                         remote_public_key: &PublicKey,
                                         messages: Vec<NetworkerTCMessage>) {

    }

    fn max_remote_debts(&self) -> Vec<u128> {
        self.token_channels.keys().map(|key| self.remote_max_debt(key).unwrap()).collect()
    }

    fn local_max_debt(&self, key: &PublicKey, channel_index: usize) -> Option<u64>{
        let tcs = self.token_channels.get(key)?;
        let channel = &tcs[channel_index];
        Some(channel.get_local_max_debt())
    }

    fn remote_max_debt(&self, remote_public_key: &PublicKey) -> Option<u128>{
        let src_tcs = self.token_channels.get(remote_public_key)?;
        let iter = src_tcs.into_iter().map(|tc| tc.get_remote_max_debt());
        Some(sum_u64(iter))
    }

    fn maximal_local_pending(&self, src: &PublicKey, dst: &PublicKey, dest_index: usize) -> Option<u64>{
        let src_max_remote_debt= self.remote_max_debt(src)?;
        let dst_max_local_debt = self.local_max_debt(dst, dest_index)?;

        Some(maximal_local_pending(self.max_remote_debts(),
                              dst_max_local_debt, src_max_remote_debt))
    }
}


fn sum_u64<I>(vals: I) -> u128
where
    I: Iterator<Item = u64>,
{
    vals.fold(0u128, |mut sum, val| sum + u128::from(val))
}



/// Assume
/// Maximal amount of credits `src` can freeze towards `dest`.
///     A -- B -- C
/// Assume `A` sends a request through `B` and `C`. Then `A` freezes some credits on the edge (B, C).
/// This function bounds that amount of credits, when `dest` is C, and `src` is A.
fn maximal_local_pending(max_remote_debts: Vec<u128>, dest_max_local_debt: u64,
                          src_max_remote_debt: u128) -> u64{
    dest_max_local_debt / 4
//    let total_mr = sum_u64_vec(max_remote_debts);
//    let trust = dest_max_remote_debt as u128 * src_max_remote_debt as u128;
//    trust / total_mr
}


#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn test_sum(){
        let v = vec![(1u64<<62), (1u64<<62), (1u64<<62), (1u64<<62)];
        assert_eq!(1u128 << 64, sum_u64(v.into_iter()));
    }

}
