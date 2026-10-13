use std::path::PathBuf;

macro_rules! usize_from_u32 {
    ($value:expr) => {{
        let value: u32 = $value;

        // Assuming we're on a 32-bit or 64-bit platform, converting from a u32 to a usize will
        // always succeed. However, we don't want to have to litter the code with `unwrap`,
        // `unreachable!()`, etc. Putting the conditional compilation only inside the error path
        // handles both generating a compiler error on unsupported platforms and eliminating any
        // need to handle the error.
        usize::try_from(value).unwrap_or_else(|_| {
            static_assertions_next::assert_cfg!(any(
                target_pointer_width = "32",
                target_pointer_width = "64"
            ));
            unreachable!();
        })
    }};
}

macro_rules! u64_from_usize {
    ($value:expr) => {{
        let value: usize = $value;

        // Assuming we're on a 32-bit or 64-bit platform, converting from a u32 to a usize will
        // always succeed. However, we don't want to have to litter the code with `unwrap`,
        // `unreachable!()`, etc. Putting the conditional compilation only inside the error path
        // handles both generating a compiler error on unsupported platforms and eliminating any
        // need to handle the error.
        u64::try_from(value).unwrap_or_else(|_| {
            static_assertions_next::assert_cfg!(any(
                target_pointer_width = "32",
                target_pointer_width = "64"
            ));
            unreachable!();
        })
    }};
}

macro_rules! try_as_metadata_size {
    (
        $input_value: expr =>
        $target_ty: ty,
        $error: ident,
        $context: expr $(,)?
    ) => {
        <$target_ty>::try_from($input_value).map_err(|_| crate::error::Error::$error {
            context: $context.into(),
            value: $input_value,
            bit_width: <$target_ty>::BITS,
        })
    };
}

macro_rules! try_as_decompression_size {
    (
        $input_value: expr =>
        $target_ty: ty,
        $context: expr $(,)?
    ) => {
        try_as_metadata_size!(
            $input_value => $target_ty,
            DecompressedFileSizeTooHigh,
            $context,
        )
    };
}

macro_rules! try_as_compression_size {
    (
        $input_value: expr =>
        $target_ty: ty,
        $context: expr $(,)?
    ) => {
        try_as_metadata_size!(
            $input_value => $target_ty,
            CompressedFileSizeTooHigh,
            $context,
        )
    };
}

pub fn pathbuf_from_bytes(bytes: &[u8]) -> PathBuf {
    String::from_utf8_lossy(bytes).into_owned().into()
}
