use std::mem;
use std::cmp::Ordering;

pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
define_fixed_bytes!(ChannelToken, CHANNEL_TOKEN_LEN);


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinearSendPrice<T> {
    pub base: T,
    pub multiplier: T,
}

impl<T> LinearSendPrice<T> {
    pub fn bytes_count() -> usize {
        mem::size_of::<T>() * 2
    }
}

impl<T:PartialOrd> PartialOrd for LinearSendPrice<T> {
    fn partial_cmp(&self, other: &LinearSendPrice<T>) -> Option<Ordering> {
        if (self.base < other.base) && (self.multiplier < other.multiplier) {
            Some(Ordering::Less)
        } else {
            None
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Debug)]
pub struct NetworkerSendPrice(pub LinearSendPrice<u32>);

impl NetworkerSendPrice {
    pub fn bytes_count() -> usize {
        LinearSendPrice::<u32>::bytes_count()
    }

    pub fn calc_cost(&self, length: u32) -> Option<u64> {
        u64::from(self.0.multiplier).checked_mul(u64::from(length))?
            .checked_add(u64::from(self.0.base))
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_send_price_ord() {
        let lsp1 = LinearSendPrice {
            base: 3,
            multiplier: 3,
        };

        let lsp2 = LinearSendPrice {
            base: 5,
            multiplier: 5,
        };

        let lsp3 = LinearSendPrice {
            base: 4,
            multiplier: 6,
        };

        assert!(lsp1 < lsp2);
        assert!(lsp1 < lsp3);

        assert!(!(lsp2 < lsp3));
        assert!(!(lsp3 < lsp2));
    }

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

