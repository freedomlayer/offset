//! The implementation of the sliding window

use byteorder::{ByteOrder, LittleEndian};

/// A sliding window
///
/// # Introduction
///
/// The slidable bit window, with a internal nonce, the inner data layout can
/// be shown as follow:
///
/// ```ignore
///                                   ...   7   6   5   4   3   2   1   0
/// +---+---+---+---+---+---+---+---+ ... +---+---+---+---+---+---+---+---+
/// | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 1 | ... | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 1 |
/// +---+---+---+---+---+---+---+---+ ... +---+---+---+---+---+---+---+---+
///   ↳  counter - width - 1                                   counter ↵
/// ```
pub struct SlidingWindow {
    blocks: Vec<u64>,
    nonce: u128,
}

impl SlidingWindow {
    /// Constructs a new `SlidingWindow` with exactly the width.
    ///
    /// # Panics
    ///
    /// Panics if the width not divisible by `64`.
    pub fn new(width: usize) -> SlidingWindow {
        assert_eq!(width % 64, 0, "width should divisible by 64");

        SlidingWindow {
            blocks: vec![0; width / 64],
            nonce: 0,
        }
    }

    /// Try to accept a nonce, returns `true` if the `nonce` was accepted.
    ///
    /// This function determine whether we should accept a nonce, we accept
    /// a nonce if it satisfies either:
    ///
    /// - The nonce greater than the old one.
    /// - The nonce in the range of `(old nonce - capacity , old nonce)`, and
    ///   it have not been accepted previous.
    ///
    /// # Panics
    ///
    /// Panics if the length of the `nonce` greater than `16`.
    pub fn try_accept(&mut self, nonce: &[u8]) -> bool {
        let nonce = nonce_to_u128(nonce);

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

/// Convert an array style nonce to `u128`.
///
/// # Panics
///
/// Panics if the length of the `nonce` greater than `16`.
#[inline]
fn nonce_to_u128(nonce: &[u8]) -> u128 {
    assert!(nonce.len() <= 16, "nonce overflows");

    let mut aligned = Vec::from(nonce);
    aligned.resize(16, 0);

    LittleEndian::read_u128(&aligned)
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

    const WINDOW_WIDTH: usize = 256;

    /// Increase the bytes represented number by 1.
    fn increase_nonce(nonce: &mut [u8]) {
        let mut c: u16 = 1;
        for i in nonce {
            c += u16::from(*i);
            *i = c as u8;
            c >>= 8;
        }
    }

    #[test]
    fn nonce_to_u128_small() {
        let nonce1 = vec![0, 1, 2];
        assert_eq!(nonce_to_u128(&nonce1), 131_328);

        let nonce2 = vec![255; 16];
        assert_eq!(nonce_to_u128(&nonce2), u128::max_value());
    }

    #[test]
    #[should_panic]
    fn nonce_to_u128_large() {
        let _ = nonce_to_u128(&vec![0; 17]);
    }

    #[test]
    fn test_shift_left() {
        let mut blocks = vec![0xf0f0f0f0_f0f0f0f0, 0xf0f0f0f0_f0f0f0f0];

        shift_left(&mut blocks, 0);
        assert_eq!(blocks, vec![0xf0f0f0f0_f0f0f0f0, 0xf0f0f0f0_f0f0f0f0]);

        shift_left(&mut blocks, 1);
        assert_eq!(blocks, vec![0xe1e1e1e1_e1e1e1e1, 0xe1e1e1e1_e1e1e1e0]);

        shift_left(&mut blocks, 15);
        assert_eq!(blocks, vec![0xf0f0f0f0_f0f0f0f0, 0xf0f0f0f0_f0f00000]);

        shift_left(&mut blocks, 31);
        assert_eq!(blocks, vec![0x78787878_78787878, 0x78780000_00000000]);

        shift_left(&mut blocks, 64);
        assert_eq!(blocks, vec![0x78780000_00000000, 0x00000000_00000000]);

        shift_left(&mut blocks, 128);
        assert_eq!(blocks, vec![0x00000000_00000000, 0x00000000_00000000]);
    }

    #[test]
    fn test_sliding_window() {
        let mut window = SlidingWindow::new(WINDOW_WIDTH);

        let mut nonce = vec![0, 0, 0, 0];

        for _ in 0..WINDOW_WIDTH {
            increase_nonce(&mut nonce);
            assert!(window.try_accept(&nonce) && !window.try_accept(&nonce));
        }

        assert_eq!(window.nonce, nonce_to_u128(&nonce));
        assert_eq!(window.blocks, vec![u64::max_value(); 4]);

        increase_nonce(&mut nonce);
        let out_of_range = nonce.clone();
        assert_eq!(out_of_range, vec![1, 1, 0, 0]);

        for _ in 0..WINDOW_WIDTH {
            increase_nonce(&mut nonce);
            assert!(window.try_accept(&nonce) && !window.try_accept(&nonce));
        }

        assert!(!window.try_accept(&out_of_range));
    }
}
