use std::mem::size_of;

/// Preallocation sizing for lists and maps that does not trust the length hint of a document.
pub struct Capacity;

impl Capacity {
    /// The most memory one preallocation may claim: 1 MiB.
    const MAX_BYTES: usize = 1024 * 1024;

    /// Returns how many `T` items to preallocate for `hint`, capped at [`Self::MAX_BYTES`] worth
    /// of items; a missing hint and a zero-sized `T` give 0.
    pub fn cautious<T>(hint: Option<usize>) -> usize {
        match size_of::<T>() {
            0 => 0,
            size => hint.unwrap_or(0).min(Self::MAX_BYTES / size),
        }
    }
}
