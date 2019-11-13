// A shim allowing to have Never type on stable rust.
// Should be removed once never type reaches stable.
//
// Based on: https://github.com/CodeChain-io/rust-never-type/blob/master/src/lib.rs

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Never {}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    #[test]
    fn size_of_never_is_zero() {
        assert_eq!(0, size_of::<Never>());
    }
}
