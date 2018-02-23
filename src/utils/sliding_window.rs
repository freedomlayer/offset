use std::cmp::Ordering;

const BASE: i16 = 256;

/// Default counter length, in the number of bytes
const DEFAULT_COUNTER_LENGTH: usize = 12;

/// Default size of window, in the number of bits
const DEFAULT_WINDOW_CAPACITY: usize = 256;

pub struct SlidingWindow {
    words:    Vec<usize>,
    counter:  Vec<u8>,
    capacity: usize,
}

/// A sliding window
///
/// The slidable bit window, with a internal counter, the inner data layout can
/// be shown as follow:
///
/// ```ignore
///                                   ...   7   6   5   4   3   2   1   0
/// +---+---+---+---+---+---+---+---+ ... +---+---+---+---+---+---+---+---+
/// | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 1 | ... | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 1 |
/// +---+---+---+---+---+---+---+---+ ... +---+---+---+---+---+---+---+---+
///   ↳  counter - capacity -1                                  counter ↵
/// ```
impl SlidingWindow {
    pub fn new() -> SlidingWindow {
        let word_size = ::std::mem::size_of::<usize>() * 8;

        SlidingWindow {
            words:    vec![usize::max_value(); DEFAULT_WINDOW_CAPACITY / word_size],
            counter:  vec![0; DEFAULT_COUNTER_LENGTH],
            capacity: DEFAULT_WINDOW_CAPACITY,
        }
    }

    /// Try to accept a new counter.
    ///
    /// Check whether the given `counter` in the range of the current
    /// window, if so, the counter will be updated and return `true`,
    /// otherwise, `false` will be returned.
    ///
    /// **NOTE:** If the length of the given counter differ to the one
    /// of window, `false` would be returned directly.
    ///
    /// # Panics
    ///
    /// Panics if the counters have different length.
    pub fn try_accept(&mut self, counter: &[u8]) -> bool {
        if counter.len() != self.counter.len() {
            return false;
        }

        match cmp(&counter, &self.counter) {
            Ordering::Equal => false,
            Ordering::Less => {
                let dif = sub_unchecked(&self.counter, &counter);
                let dif = dif.iter()
                    .rev()
                    .fold(0usize, |dif, &x| dif * BASE as usize + x as usize);

                dif < self.capacity && !self.set(dif)
            }
            Ordering::Greater => {
                let dif = sub_unchecked(&counter, &self.counter);
                if dif.len() > ::std::mem::size_of::<usize>() {
                    self.clear();
                } else {
                    let dif = dif.iter()
                        .rev()
                        .fold(0usize, |dif, &x| dif * BASE as usize + x as usize);
                    if dif >= self.capacity {
                        self.clear();
                    } else {
                        self.shift_left(dif);
                    }
                }

                self.counter.copy_from_slice(&counter);
                !self.set(0)
            }
        }
    }

    fn clear(&mut self) {
        self.words = vec![0; self.capacity / (::std::mem::size_of::<usize>() * 8)];
    }

    fn shift_left(&mut self, nbits: usize) {
        let word_size = ::std::mem::size_of::<usize>() * 8;

        let nwords = nbits / word_size;
        self.words = self.words.split_off(nwords);
        self.words.extend(&vec![0; nwords]);

        assert_eq!(
            self.words.len(),
            self.capacity / word_size,
            "bad shift left in words"
        );

        let nbits = nbits % word_size;

        let len = self.words.len();

        for i in 0..(len - 1) {
            self.words[i] <<= nbits;
            self.words[i] |= self.words[i + 1] >> (word_size - nbits);
        }
        self.words[len - 1] <<= nbits;
    }

    // Set the specified bit, return the previous value
    fn set(&mut self, i: usize) -> bool {
        assert!(
            i < self.capacity,
            "index out of bounds: {} >= {}",
            i,
            self.capacity
        );

        let (w, f) = calc_offset_and_flag(i, self.capacity);

        let prev = self.words[w] & f == f;

        self.words[w] |= f;

        prev
    }
}

#[inline]
fn calc_offset_and_flag(mut i: usize, capacity: usize) -> (usize, usize) {
    let word_size = ::std::mem::size_of::<usize>() * 8;

    i = capacity - 1 - i;

    let w = i / word_size;
    let b = i % word_size;

    let f = 1 << (word_size - 1 - b);

    (w, f)
}

#[inline]
fn cmp(lhs: &[u8], rhs: &[u8]) -> Ordering {
    assert_eq!(lhs.len(), rhs.len());

    let len = lhs.len();

    for i in (0..len).rev() {
        match lhs[i].cmp(&rhs[i]) {
            Ordering::Equal => (),
            res @ _ => return res,
        }
    }

    Ordering::Equal
}

#[inline]
fn sub_unchecked(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let len = ::std::cmp::min(lhs.len(), rhs.len());
    let (lhs_lo, lhs_hi) = lhs.split_at(len);
    let (rhs_lo, rhs_hi) = rhs.split_at(len);

    let mut dif: i16 = 0;
    let mut res = Vec::new();

    for (lhs, rhs) in lhs_lo.iter().zip(rhs_lo) {
        dif += *lhs as i16;
        dif -= *rhs as i16;

        if dif < 0 {
            res.push((dif + BASE) as u8);
            dif = -1;
        } else {
            res.push((dif % BASE) as u8);
            dif = dif / BASE;
        }
    }

    for lhs in lhs_hi {
        dif += *lhs as i16;

        res.push((dif % BASE) as u8);
        dif = dif / BASE;
    }

    assert!(
        dif == 0 && rhs_hi.iter().all(|rhs| *rhs == 0),
        "can't not subtract rhs from lhs because rhs is larger than lhs"
    );

    // Remove the trailing ZERO
    while res.ends_with(&[0]) {
        res.pop();
    }

    res
}

/// Increase the bytes represented number by 1.
///
/// Reference: `libsodium/sodium/utils.c#L241`
#[inline]
#[cfg(test)]
fn increase_nonce(nonce: &mut [u8]) {
    let mut c: u16 = 1;
    for i in nonce {
        c += u16::from(*i);
        *i = c as u8;
        c >>= 8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_unchecked_same_length() {
        let a = vec![0, 0, 0];
        let b = vec![0, 0, 0];

        assert_eq!(sub_unchecked(&a, &b), vec![]);

        let a = vec![4, 5, 6];
        let b = vec![1, 2, 3];

        assert_eq!(sub_unchecked(&a, &b), vec![3, 3, 3]);

        let a = vec![7, 8, 9];
        let b = vec![9, 8, 7];

        assert_eq!(sub_unchecked(&a, &b), vec![254, 255, 1]);
    }

    #[test]
    fn test_sub_unchecked_diff_length() {
        let a = vec![0, 0];
        let b = vec![0, 0, 0];

        assert_eq!(sub_unchecked(&a, &b), vec![]);

        let a = vec![4, 5, 6, 7];
        let b = vec![1, 2, 3];

        assert_eq!(sub_unchecked(&a, &b), vec![3, 3, 3, 7]);

        let a = vec![1, 2, 3, 4];
        let b = vec![2, 3, 4];

        assert_eq!(sub_unchecked(&a, &b), vec![255, 254, 254, 3]);

        let a = vec![1, 2, 3, 4];
        let b = vec![2, 3, 4, 0, 0];

        assert_eq!(sub_unchecked(&a, &b), vec![255, 254, 254, 3]);
    }

    #[test]
    fn test_sliding_window() {
        let mut window = SlidingWindow::new();

        let zero = vec![0x00; DEFAULT_COUNTER_LENGTH];
        let mut counter = vec![0x00; DEFAULT_COUNTER_LENGTH];

        // Don't accept the zero
        assert!(!window.try_accept(&zero));

        for i in 0..DEFAULT_WINDOW_CAPACITY {
            increase_nonce(&mut counter);
            assert!(window.try_accept(&counter) && !window.try_accept(&counter));
        }

        let word_size = ::std::mem::size_of::<usize>() * 8;
        assert_eq!(window.counter, counter);
        assert_eq!(
            window.words,
            vec![usize::max_value(); DEFAULT_WINDOW_CAPACITY / word_size]
        );

        // Don't accept the zero
        assert!(!window.try_accept(&zero));

        increase_nonce(&mut counter);
        let delay_counter = counter.clone();

        for i in 1..DEFAULT_WINDOW_CAPACITY {
            increase_nonce(&mut counter);
            assert!(window.try_accept(&counter) && !window.try_accept(&counter));
        }

        assert!(window.try_accept(&delay_counter) && !window.try_accept(&delay_counter));
    }
}
