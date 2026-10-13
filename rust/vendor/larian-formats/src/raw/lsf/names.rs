use crate::error::{Error, Result};
use std::{
    io::{Read, Write},
    mem::size_of,
    ops::Index,
};

use super::{AttributeInfo, NodeInfo};

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Clone))]
pub(super) struct Names {
    pub(super) inner: Vec<Vec<String>>,
}

macro_rules! write_len_bytes {
    (  $bytes:expr, $int:ty, $size:expr, $context:expr, $error:ident ) => {{
        let value = u64_from_usize!($size.len());
        let num = try_as_metadata_size!(
            value => $int,
            $error,
            $context,
        )?;

        $bytes
            .write_all(&num.to_le_bytes())
            .map_err(|e| Error::write_io($context, e))?;
    }};
}

impl Names {
    pub(super) fn byte_len(&self) -> usize {
        size_of::<u32>() // outer bucket count
            + self.inner.iter().map(|bucket| {
                size_of::<u16>() // inner string count
                    + bucket.iter().map(|s| {
                        size_of::<u16>() // string byte length
                            + s.len()
                    }).sum::<usize>()
            }).sum::<usize>()
    }

    pub(super) fn write(self, output: &mut impl Write) -> Result<()> {
        write_len_bytes!(output, u32, self.inner, ".lsf names".to_string(), NumberOfEntriesTooHigh);

        for (outer_idx, inner_vec) in self.inner.iter().enumerate() {
            write_len_bytes!(
                output,
                u16,
                inner_vec,
                format!(".lsf names entry {outer_idx}"),
                NumberOfEntriesTooHigh
            );

            for (inner_idx, string) in inner_vec.iter().enumerate() {
                let string_bytes = string.as_bytes();
                write_len_bytes!(
                    output,
                    u16,
                    string_bytes,
                    format!(".lsf names entry {outer_idx} string {inner_idx}"),
                    CompressedFileSizeTooHigh
                );
                output.write_all(string_bytes).map_err(|e| {
                    Error::write_io(format!(".lsf names[{outer_idx}][{inner_idx}] entry bytes"), e)
                })?;
            }
        }

        Ok(())
    }

    pub(super) fn from_bytes(mut input: &[u8]) -> Result<Self> {
        let input = &mut input;

        let num_outer = read_num!(u32, input, ".lsf number of names")?;
        let inner: Result<_> = std::iter::repeat_with(|| {
            let num_inner =
                read_num!(u16, input, format!(".lsf number of strings in entry {num_outer}"))?;

            std::iter::repeat_with(|| {
                let len = read_num!(
                    u16,
                    input,
                    format!(".lsf number of bytes in entry {num_outer} string index {num_inner}")
                )?;
                let mut bytes = vec![0; len.into()];
                input.read_exact(&mut bytes).map_err(|e| {
                    Error::read_io(".lsf number of bytes in entry {num_outer} string index", e)
                })?;

                Ok(String::from_utf8_lossy(&bytes).into())
            })
            .take(num_inner.into())
            .collect()
        })
        .take(usize_from_u32!(num_outer))
        .collect();

        Ok(Self { inner: inner? })
    }
}

impl Index<&NodeInfo> for Names {
    type Output = str;

    fn index(&self, info: &NodeInfo) -> &Self::Output {
        &self.inner[usize::from(info.entry_outer_index)][usize::from(info.entry_inner_index)]
    }
}

impl Index<&AttributeInfo> for Names {
    type Output = str;

    fn index(&self, info: &AttributeInfo) -> &Self::Output {
        &self.inner[usize::from(info.entry_outer_index)][usize::from(info.entry_inner_index)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_casing::test_casing;
    const ROUND_TRIP_CASES: &[&[&[&str]]] = &[
        &[],
        &[&[]],
        &[&["test"]],
        &[&["foo", "bar"], &["baz"]],
        &[&["hello", "world"], &[], &["rust"]],
    ];

    #[test_casing(5, ROUND_TRIP_CASES)]
    fn test_names_round_trip(&data: &&[&[&str]]) {
        let names = Names {
            inner: data
                .iter()
                .map(|outer| outer.iter().map(ToString::to_string).collect())
                .collect(),
        };

        let mut bytes = Vec::new();
        names.clone().write(&mut bytes).unwrap();
        let parsed = Names::from_bytes(&bytes).unwrap();

        assert_eq!(names, parsed);
    }
}
