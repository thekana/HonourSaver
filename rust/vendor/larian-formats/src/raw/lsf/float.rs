static F32_LOG10_BOUNDS: (f32, f32) = (-4.0, 7.0);
static F64_LOG10_BOUNDS: (f64, f64) = (-4.0, 15.0);

macro_rules! fmt_float {
    ($v:expr, $bounds:expr) => {{
        use std::num::FpCategory;

        let log = $v.abs().log10();
        let category = $v.classify();

        if category == FpCategory::Zero ||
            (category == FpCategory::Normal && (log >= $bounds.0 && log < $bounds.1))
        {
            if $v.fract() == 0.0 {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "no other way to convert from f32"
                )]
                {
                    format!("{}", $v.trunc() as i64)
                }
            } else {
                $v.to_string()
            }
        } else {
            let mut s = format!("{:E}", $v);

            // Safe cast: finite f64 values have log10 in [-324, 308], which fits in i16
            // (max ±32767). i8 is too small (max 127 < 308). Inf/NaN format without 'E'
            // so the find below won't match for those.
            #[expect(clippy::cast_possible_truncation, reason = "will always fit")]
            let exp = log.floor() as i16;
            let sign = if exp >= 0 { "+" } else { "-" };
            let exp_abs = exp.unsigned_abs();
            let padded = if exp_abs < 10 {
                format!("0{exp_abs}")
            } else {
                format!("{exp_abs}")
            };
            if let Some(e_pos) = s.find('E') {
                s = format!("{}E{sign}{padded}", &s[..e_pos]);
            }

            s
        }
    }};
}

pub(super) fn format_f32_lslib(value: f32) -> String {
    fmt_float!(value, F32_LOG10_BOUNDS)
}

pub(super) fn format_f64_lslib(value: f64) -> String {
    fmt_float!(value, F64_LOG10_BOUNDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_exponent_has_plus_sign() {
        assert_eq!(format_f32_lslib(1e7_f32), "1E+07");
        assert_eq!(format_f64_lslib(1e15_f64), "1E+15");
    }

    #[test]
    fn negative_exponent_has_minus_sign() {
        assert_eq!(format_f32_lslib(1e-5_f32), "1E-05");
        assert_eq!(format_f64_lslib(1e-5_f64), "1E-05");
    }
}
