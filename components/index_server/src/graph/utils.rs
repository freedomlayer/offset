/// An iterator that wraps an optional iterator.
/// If the optional iterator exists, OptionIterator will behave exactly like the underlying
/// iterator. Otherwise, it will immediately return None.
///
/// This iterator is useful for cases where we want a function to be able to either return an
/// Iterator, or an empty Iterator, and those two iterators must be of the same type. Useful for
/// functions that return `impl Iterator<...>`.
pub struct OptionIterator<I> {
    opt_iterator: Option<I>,
}

impl<I, T> Iterator for OptionIterator<I>
where
    I: Iterator<Item = T>,
{
    type Item = T;
    fn next(&mut self) -> Option<T> {
        match &mut self.opt_iterator {
            Some(iterator) => iterator.next(),
            None => None,
        }
    }
}

impl<I> OptionIterator<I> {
    pub fn new(opt_iterator: Option<I>) -> OptionIterator<I> {
        OptionIterator { opt_iterator }
    }
}

/// Util function to convert Option<T> to Vec<T>.
/// Some(t) => vec![t], None => vec![]
pub fn option_to_vec<T>(opt_t: Option<T>) -> Vec<T> {
    match opt_t {
        Some(t) => vec![t],
        None => vec![],
    }
}

// TODO: add tests
