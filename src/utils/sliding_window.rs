//! The implementation of the sliding window

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
    words: Vec<u64>,
    nonce: u128,
    width: usize,
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
            words: vec![0; width / 64],
            nonce: 0,
            width,
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

        if nonce < self.nonce {
            let dif = self.nonce - nonce;

            if dif >= self.width as u128 {
                false
            } else {
                !self.set(dif as usize)
            }
        } else {
            let dif = nonce - self.nonce;

            // Note: we truncate the difference here
            self.shift_left(::std::cmp::min(dif, self.width as u128) as usize);

            self.nonce = nonce;
            !self.set(0)
        }
    }

    /// Bitwise shift left.
    fn shift_left(&mut self, nbits: usize) {
        if nbits > 0 {
            if nbits >= self.width {
                self.words = vec![0; self.width / 64];
            } else {
                let nwords = nbits / 64;
                self.words = self.words.split_off(nwords);
                self.words.extend(&vec![0; nwords]);

                assert_eq!(self.words.len(), self.width / 64, "bad shift result");

                let nbits = nbits % 64;
                if nbits > 0 {
                    let len = self.words.len();

                    for i in 0..(len - 1) {
                        self.words[i] <<= nbits;
                        self.words[i] |= self.words[i + 1] >> (64 - nbits);
                    }
                    self.words[len - 1] <<= nbits;
                }
            }
        }
    }

    /// Set the specified bit, returns the old value.
    fn set(&mut self, i: usize) -> bool {
        assert!(i < self.width, "index out of bounds: {} >= {}", i, self.width);

        let i = self.width - 1 - i;

        let mask = 1 << (64 - 1 - i % 64);
        let old_bit = self.words[i / 64] & mask == mask;
        self.words[i / 64] |= mask;

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
    assert!(nonce.len() <= 16, "nonce length overflows");

    let mut aligned = Vec::from(nonce);
    aligned.extend(&vec![0; 16 - nonce.len()]);

    #[cfg(target_endian="big")]
    aligned.reverse();

    unsafe {
        let u128_ptr = ::std::mem::transmute::<*const u8, *const u128>(aligned.as_ptr());

        *u128_ptr
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
    fn test_sliding_window_shift() {
        let mut window = SlidingWindow::new(WINDOW_WIDTH);
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
        assert_eq!(window.words, vec![u64::max_value(); 4]);

        increase_nonce(&mut nonce);
        let out_of_range = nonce.clone();
        assert_eq!(out_of_range, vec![1, 1, 0, 0]);

        for i in 0..WINDOW_WIDTH {
            increase_nonce(&mut nonce);
            assert!(window.try_accept(&nonce) && !window.try_accept(&nonce));
        }

        assert!(!window.try_accept(&out_of_range));
    }
}
