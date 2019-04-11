//! Implementation of the sliding nonce window

/// A sliding nonce window
///
/// The sliding nonce window keeps a nonce, which indicates the upper
/// bound of the window. The inner data layout can be shown as follow:
///
/// ```notrust
///                                   ...   7   6   5   4   3   2   1   0
/// +---+---+---+---+---+---+---+---+ ... +---+---+---+---+---+---+---+---+
/// | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 1 | ... | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 1 |
/// +---+---+---+---+---+---+---+---+ ... +---+---+---+---+---+---+---+---+
///   ↳  nonce - width - 1                                       nonce ↵
/// ```
pub struct NonceWindow {
    nonce: u128,
    blocks: Vec<u64>,
}

pub trait WindowNonce: Into<u128> {}

impl NonceWindow {
    /// Constructs a new `NonceWindow` with exactly the width.
    ///
    /// # Panics
    ///
    /// Panics if the width not divisible by `64`.
    pub fn new(width: usize) -> NonceWindow {
        assert_eq!(width % 64, 0, "width should divisible by 64");

        NonceWindow {
            nonce: 0,
            blocks: vec![0; width / 64],
        }
    }

    /// Try to accept a nonce, returns `true` if the `nonce` was accepted.
    ///
    /// This function determine whether we should accept a nonce, we accept
    /// a nonce if it satisfies either of:
    ///
    /// - The nonce greater than the old one.
    /// - The nonce in the range of `(old nonce - window width, old nonce]`,
    ///   and it have not been accepted previous.
    pub fn try_accept<T: WindowNonce>(&mut self, nonce: T) -> bool {
        let nonce = nonce.into();

        let width = self.blocks.len() as u128 * 64;

        if nonce < self.nonce {
            let dif = self.nonce - nonce;

            if dif >= width {
                false
            } else {
                !self.set(dif as usize)
            }
        } else {
            let dif = nonce - self.nonce;

            // Note: we truncate the difference here
            shift_left(&mut self.blocks, ::std::cmp::min(dif, width) as usize);

            self.nonce = nonce;
            !self.set(0)
        }
    }

    /// Set the specified bit, returns the old value.
    fn set(&mut self, i: usize) -> bool {
        let width = self.blocks.len() * 64;

        assert!(i < width, "index out of bounds: {} >= {}", i, width);

        let i = width - 1 - i;

        let mask = 1 << (64 - 1 - i % 64);
        let old_bit = self.blocks[i / 64] & mask == mask;
        self.blocks[i / 64] |= mask;

        old_bit
    }
}

/// Logical left shift of the bits.
#[inline]
fn shift_left(blocks: &mut Vec<u64>, nbits: usize) {
    let len = blocks.len();

    if nbits > 0 {
        if nbits >= len * 64 {
            *blocks = vec![0; len];
        } else {
            let nwords = nbits / 64;
            *blocks = blocks.split_off(nwords);
            blocks.extend(&vec![0; nwords]);

            let nbits = nbits % 64;
            if nbits > 0 {
                for i in 0..(len - 1) {
                    blocks[i] <<= nbits;
                    blocks[i] |= blocks[i + 1] >> (64 - nbits);
                }
                blocks[len - 1] <<= nbits;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::increase_nonce;
    use byteorder::{ByteOrder, LittleEndian};

    const NONCE_LENGTH: usize = 4;
    const WINDOW_WIDTH: usize = 256;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Nonce(pub [u8; NONCE_LENGTH]);

    impl<'a> From<&'a Nonce> for u128 {
        #[inline]
        fn from(src: &'a Nonce) -> u128 {
            let mut aligned = Vec::from(&src.0[..]);
            aligned.resize(16, 0);

            LittleEndian::read_u128(&aligned)
        }
    }

    impl<'a> WindowNonce for &'a Nonce {}

    #[test]
    fn test_shift_left() {
        let mut blocks = vec![0xf0f0_f0f0_f0f0_f0f0, 0xf0f0_f0f0_f0f0_f0f0];

        shift_left(&mut blocks, 0);
        assert_eq!(blocks, vec![0xf0f0_f0f0_f0f0_f0f0, 0xf0f0_f0f0_f0f0_f0f0]);

        shift_left(&mut blocks, 1);
        assert_eq!(blocks, vec![0xe1e1_e1e1_e1e1_e1e1, 0xe1e1_e1e1_e1e1_e1e0]);

        shift_left(&mut blocks, 15);
        assert_eq!(blocks, vec![0xf0f0_f0f0_f0f0_f0f0, 0xf0f0_f0f0_f0f0_0000]);

        shift_left(&mut blocks, 31);
        assert_eq!(blocks, vec![0x7878_7878_7878_7878, 0x7878_0000_0000_0000]);

        shift_left(&mut blocks, 64);
        assert_eq!(blocks, vec![0x7878_0000_0000_0000, 0x0000_0000_0000_0000]);

        shift_left(&mut blocks, 128);
        assert_eq!(blocks, vec![0x0000_0000_0000_0000, 0x0000_0000_0000_0000]);
    }

    #[test]
    fn test_nonce_window() {
        let mut window = NonceWindow::new(WINDOW_WIDTH);

        let mut nonce = Nonce([0; NONCE_LENGTH]);

        for _ in 0..WINDOW_WIDTH {
            increase_nonce(&mut nonce.0[..]);
            assert!(window.try_accept(&nonce) && !window.try_accept(&nonce));
        }

        assert_eq!(window.nonce, (&nonce).into());
        assert_eq!(window.blocks, vec![u64::max_value(); WINDOW_WIDTH / 64]);

        increase_nonce(&mut nonce.0[..]);
        let out_of_range = nonce.clone();
        assert_eq!(out_of_range, Nonce([1, 1, 0, 0]));

        for _ in 0..WINDOW_WIDTH {
            increase_nonce(&mut nonce.0[..]);
            assert!(window.try_accept(&nonce) && !window.try_accept(&nonce));
        }

        assert!(!window.try_accept(&out_of_range));
    }
}
