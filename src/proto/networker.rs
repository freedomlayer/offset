use byteorder::{WriteBytesExt, BigEndian};

use super::LinearSendPrice;

pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
define_fixed_bytes!(ChannelToken, CHANNEL_TOKEN_LEN);



#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NetworkerSendPrice(pub LinearSendPrice<u32>);


impl NetworkerSendPrice {
    pub fn bytes_count() -> usize {
        LinearSendPrice::<u32>::bytes_count()
    }

    pub fn calc_cost(&self, length: u32) -> Option<u64> {
        u64::from(self.0.multiplier).checked_mul(u64::from(length))?
            .checked_add(u64::from(self.0.base))
    }

    pub fn smaller_than(&self, other: &Self) -> bool {
        self.0.smaller_than(&other.0)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.write_u32::<BigEndian>(self.0.base).expect("Serialization failure!");
        res_bytes.write_u32::<BigEndian>(self.0.multiplier).expect("Serialization failure!");
        res_bytes
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_networker_send_price_calc_cost_basic() {
        let nsp = NetworkerSendPrice(LinearSendPrice {
            base: 5,
            multiplier: 3,
        });
        assert_eq!(nsp.calc_cost(0), Some(5));
        assert_eq!(nsp.calc_cost(1), Some(8));
    }
}

