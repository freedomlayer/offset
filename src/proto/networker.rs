use std::mem;

pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
define_fixed_bytes!(ChannelToken, CHANNEL_TOKEN_LEN);


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinearSendPrice<T> {
    pub base: T,
    pub multiplier: T,
}

impl<T:PartialOrd> LinearSendPrice<T> {
    pub fn bytes_count() -> usize {
        mem::size_of::<T>() * 2
    }

    pub fn smaller_than(&self, other: &Self) -> bool {
        if self.base < other.base {
            self.multiplier <= other.multiplier
        } else if self.base == other.base {
            self.multiplier < other.multiplier
        } else {
            false
        }
    }
}

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

        let lsp4 = LinearSendPrice {
            base: 4,
            multiplier: 7,
        };

        let lsp5 = LinearSendPrice {
            base: 5,
            multiplier: 6,
        };

        assert!(lsp1.smaller_than(&lsp2));
        assert!(lsp1.smaller_than(&lsp3));

        assert!(!(lsp2.smaller_than(&lsp3)));
        assert!(!(lsp3.smaller_than(&lsp2)));

        assert!(lsp3.smaller_than(&lsp4));
        assert!(lsp3.smaller_than(&lsp5));
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

