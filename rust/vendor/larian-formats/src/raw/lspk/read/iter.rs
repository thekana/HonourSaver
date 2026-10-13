use std::borrow::Cow;

use super::Table;

/// Contains the parsed metadata of a mod.
#[derive(Debug)]
pub struct ModFilePaths {
    pub(super) table: Table,
}

struct Paths<I> {
    iter: I,
}

impl ModFilePaths {
    /// Returns an iterator over the file paths in the mod.
    pub fn iter(&self) -> impl Iterator<Item = Cow<'_, [u8]>> {
        Paths {
            iter: self.table.paths(),
        }
    }
}

impl<I> Iterator for Paths<I>
where
    I: Iterator,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
