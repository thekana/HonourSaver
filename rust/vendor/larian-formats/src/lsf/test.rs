use crate::lsf::LsfData;
use paste::paste;
use pretty_assertions::assert_eq;

macro_rules! read_from_test_file {
    (
        $name: ident,
        $format: ident
    ) => {{
        paste! {
            let bytes = include_bytes!(
                concat!(
                    "./test-files/",
                    stringify!($name),
                    ".",
                    stringify!($format),
                )
            );

            LsfData::[<read_ $format>](&mut bytes.as_slice()).unwrap()
        }
    }};
}

macro_rules! define_roundtrip_test {
    ( 
        $name:ident, 
        $initial: ident, 
        $intermediate: ident
    ) => {
        paste! {
            #[test]
            fn [<$name _round_trip_from_ $initial>]() {
                let before = read_from_test_file!($name, $initial);

                let mut intermediate = Vec::new();
                before.[<write_ $intermediate>](&mut intermediate).unwrap();

                let after = read_from_test_file!($name, $intermediate);
                assert_eq!(before, after);
            }
        }
    };
}

macro_rules! define_tests {
    (
        $( $name:ident ),+
        $(,)?
    ) => {
        $(
            define_roundtrip_test!($name, lsx, lsf);
            define_roundtrip_test!($name, lsf, lsx);

            paste! {
                #[test]
                fn [<$name _lsf_and_lsx_parse_identically>]() {
                    let lsf = read_from_test_file!($name, lsf);
                    let lsx = read_from_test_file!($name, lsx);

                    assert_eq!(lsf, lsx);
                }
            }
        )+
    };
}

define_tests!(intermediate_text, config, guid, translated_string, no_adjacency);
