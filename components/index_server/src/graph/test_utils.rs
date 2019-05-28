use crate::graph::capacity_graph::LinearRate;

/// A contant rate, used for testing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstRate(pub u32);

#[cfg(test)]
impl LinearRate for ConstRate {
    type K = u32;

    fn zero() -> Self {
        ConstRate(0)
    }

    fn calc_fee(self, k: Self::K) -> Self::K {
        self.0
    }

    fn checked_add(self, other: &Self) -> Option<Self> {
        Some(ConstRate(self.0.checked_add(other.0)?))
    }
}
