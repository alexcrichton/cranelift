
//! Immediate operands for Cretonne instructions
//!
//! This module defines the types of immediate operands that can appear on Cretonne instructions.
//! Each type here should have a corresponding definition in the `cretonne.immediates` Python
//! module in the meta language.

use std::fmt::{self, Display, Formatter};
use std::mem;
use std::str::FromStr;

/// 64-bit immediate integer operand.
///
/// An `Imm64` operand can also be used to represent immediate values of smaller integer types by
/// sign-extending to `i64`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Imm64(i64);

impl Imm64 {
    /// Create a new `Imm64` representing the signed number `x`.
    pub fn new(x: i64) -> Imm64 {
        Imm64(x)
    }
}

impl Into<i64> for Imm64 {
    fn into(self) -> i64 {
        self.0
    }
}

impl From<i64> for Imm64 {
    fn from(x: i64) -> Self {
        Imm64(x)
    }
}

impl Display for Imm64 {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let x = self.0;
        if -10_000 < x && x < 10_000 {
            // Use decimal for small numbers.
            write!(f, "{}", x)
        } else {
            // Hexadecimal with a multiple of 4 digits and group separators:
            //
            //   0xfff0
            //   0x0001_ffff
            //   0xffff_ffff_fff8_4400
            //
            let mut pos = (64 - x.leading_zeros() - 1) & 0xf0;
            write!(f, "0x{:04x}", (x >> pos) & 0xffff)?;
            while pos > 0 {
                pos -= 16;
                write!(f, "_{:04x}", (x >> pos) & 0xffff)?;
            }
            Ok(())
        }
    }
}

impl FromStr for Imm64 {
    type Err = &'static str;

    // Parse a decimal or hexadecimal `Imm64`, formatted as above.
    fn from_str(s: &str) -> Result<Imm64, &'static str> {
        let mut value: u64 = 0;
        let mut digits = 0;
        let negative = s.starts_with('-');
        let s2 = if negative { &s[1..] } else { s };

        if s2.starts_with("0x") {
            // Hexadecimal.
            for ch in s2[2..].chars() {
                match ch.to_digit(16) {
                    Some(digit) => {
                        digits += 1;
                        if digits > 16 {
                            return Err("Too many hexadecimal digits in Imm64");
                        }
                        // This can't overflow given the digit limit.
                        value = (value << 4) | digit as u64;
                    }
                    None => {
                        // Allow embedded underscores, but fail on anything else.
                        if ch != '_' {
                            return Err("Invalid character in hexadecimal Imm64");
                        }
                    }
                }
            }
        } else {
            // Decimal number, possibly negative.
            for ch in s2.chars() {
                match ch.to_digit(16) {
                    Some(digit) => {
                        digits += 1;
                        match value.checked_mul(10) {
                            None => return Err("Too large decimal Imm64"),
                            Some(v) => value = v,
                        }
                        match value.checked_add(digit as u64) {
                            None => return Err("Too large decimal Imm64"),
                            Some(v) => value = v,
                        }
                    }
                    None => {
                        // Allow embedded underscores, but fail on anything else.
                        if ch != '_' {
                            return Err("Invalid character in decimal Imm64");
                        }
                    }
                }
            }
        }

        if digits == 0 {
            return Err("No digits in Imm64");
        }

        // We support the range-and-a-half from -2^63 .. 2^64-1.
        if negative {
            value = value.wrapping_neg();
            // Don't allow large negative values to wrap around and become positive.
            if value as i64 > 0 {
                return Err("Negative number too small for Imm64");
            }
        }
        Ok(Imm64::new(value as i64))
    }
}

/// 8-bit unsigned integer immediate operand.
///
/// This is used to indicate lane indexes typically.
pub type Uimm8 = u8;

/// An IEEE binary32 immediate floating point value.
///
/// All bit patterns are allowed.
#[derive(Copy, Clone, Debug)]
pub struct Ieee32(f32);

/// An IEEE binary64 immediate floating point value.
///
/// All bit patterns are allowed.
#[derive(Copy, Clone, Debug)]
pub struct Ieee64(f64);

// Format a floating point number in a way that is reasonably human-readable, and that can be
// converted back to binary without any rounding issues. The hexadecimal formatting of normal and
// subnormal numbers is compatible with C99 and the `printf "%a"` format specifier. The NaN and Inf
// formats are not supported by C99.
//
// The encoding parameters are:
//
// w - exponent field width in bits
// t - trailing significand field width in bits
//
fn format_float(bits: u64, w: u8, t: u8, f: &mut Formatter) -> fmt::Result {
    debug_assert!(w > 0 && w <= 16, "Invalid exponent range");
    debug_assert!(1 + w + t <= 64, "Too large IEEE format for u64");
    debug_assert!((t + w + 1).is_power_of_two(), "Unexpected IEEE format size");

    let max_e_bits = (1u64 << w) - 1;
    let t_bits = bits & ((1u64 << t) - 1); // Trailing significand.
    let e_bits = (bits >> t) & max_e_bits; // Biased exponent.
    let sign_bit = (bits >> w + t) & 1;

    let bias: i32 = (1 << (w - 1)) - 1;
    let e = e_bits as i32 - bias; // Unbiased exponent.
    let emin = 1 - bias; // Minimum exponent.

    // How many hexadecimal digits are needed for the trailing significand?
    let digits = (t + 3) / 4;
    // Trailing significand left-aligned in `digits` hexadecimal digits.
    let left_t_bits = t_bits << (4 * digits - t);

    // All formats share the leading sign.
    if sign_bit != 0 {
        write!(f, "-")?;
    }

    if e_bits == 0 {
        if t_bits == 0 {
            // Zero.
            write!(f, "0.0")
        } else {
            // Subnormal.
            write!(f, "0x0.{0:01$x}p{2}", left_t_bits, digits as usize, emin)
        }
    } else if e_bits == max_e_bits {
        if t_bits == 0 {
            // Infinity.
            write!(f, "Inf")
        } else {
            // NaN.
            let payload = t_bits & ((1 << (t - 1)) - 1);
            if t_bits & (1 << (t - 1)) != 0 {
                // Quiet NaN.
                if payload != 0 {
                    write!(f, "NaN:0x{:x}", payload)
                } else {
                    write!(f, "NaN")
                }
            } else {
                // Signaling NaN.
                write!(f, "sNaN:0x{:x}", payload)
            }
        }
    } else {
        // Normal number.
        write!(f, "0x1.{0:01$x}p{2}", left_t_bits, digits as usize, e)
    }
}

// Parse a float using the same format as `format_float` above.
//
// The encoding parameters are:
//
// w - exponent field width in bits
// t - trailing significand field width in bits
//
fn parse_float(s: &str, w: u8, t: u8) -> Result<u64, &'static str> {
    debug_assert!(w > 0 && w <= 16, "Invalid exponent range");
    debug_assert!(1 + w + t <= 64, "Too large IEEE format for u64");
    debug_assert!((t + w + 1).is_power_of_two(), "Unexpected IEEE format size");

    let (sign_bit, s2) = if s.starts_with('-') {
        (1u64 << t + w, &s[1..])
    } else {
        (0, s)
    };

    if !s2.starts_with("0x") {
        let max_e_bits = ((1u64 << w) - 1) << t;
        let quiet_bit = 1u64 << (t - 1);

        // The only decimal encoding allowed is 0.
        if s2 == "0.0" {
            return Ok(sign_bit);
        }

        if s2 == "Inf" {
            // +/- infinity: e = max, t = 0.
            return Ok(sign_bit | max_e_bits);
        }
        if s2 == "NaN" {
            // Canonical quiet NaN: e = max, t = quiet.
            return Ok(sign_bit | max_e_bits | quiet_bit);
        }
        if s2.starts_with("NaN:0x") {
            // Quiet NaN with payload.
            return match u64::from_str_radix(&s2[6..], 16) {
                       Ok(payload) if payload < quiet_bit => {
                           Ok(sign_bit | max_e_bits | quiet_bit | payload)
                       }
                       _ => Err("Invalid NaN payload"),
                   };
        }
        if s2.starts_with("sNaN:0x") {
            // Signaling NaN with payload.
            return match u64::from_str_radix(&s2[7..], 16) {
                       Ok(payload) if 0 < payload && payload < quiet_bit => {
                           Ok(sign_bit | max_e_bits | payload)
                       }
                       _ => Err("Invalid sNaN payload"),
                   };
        }

        return Err("Float must be hexadecimal");
    }
    let s3 = &s2[2..];

    let mut digits = 0u8;
    let mut digits_before_period: Option<u8> = None;
    let mut significand = 0u64;
    let mut exponent = 0i32;

    for (idx, ch) in s3.char_indices() {
        match ch {
            '.' => {
                // This is the radix point. There can only be one.
                if digits_before_period != None {
                    return Err("Multiple radix points");
                } else {
                    digits_before_period = Some(digits);
                }
            }
            'p' => {
                // The following exponent is a decimal number.
                let exp_str = &s3[1 + idx..];
                match exp_str.parse::<i16>() {
                    Ok(e) => {
                        exponent = e as i32;
                        break;
                    }
                    Err(_) => return Err("Bad exponent"),
                }
            }
            _ => {
                match ch.to_digit(16) {
                    Some(digit) => {
                        digits += 1;
                        if digits > 16 {
                            return Err("Too many digits");
                        }
                        significand = (significand << 4) | digit as u64;
                    }
                    None => return Err("Invalid character"),
                }
            }

        }
    }

    if digits == 0 {
        return Err("No digits");
    }

    if significand == 0 {
        // This is +/- 0.0.
        return Ok(sign_bit);
    }

    // Number of bits appearing after the radix point.
    match digits_before_period {
        None => {} // No radix point present.
        Some(d) => exponent -= 4 * (digits - d) as i32,
    };

    // Normalize the significand and exponent.
    let significant_bits = (64 - significand.leading_zeros()) as u8;
    if significant_bits > t + 1 {
        let adjust = significant_bits - (t + 1);
        if significand & ((1u64 << adjust) - 1) != 0 {
            return Err("Too many significant bits");
        }
        // Adjust significand down.
        significand >>= adjust;
        exponent += adjust as i32;
    } else {
        let adjust = t + 1 - significant_bits;
        significand <<= adjust;
        exponent -= adjust as i32;
    }
    assert_eq!(significand >> t, 1);

    // Trailing significand excludes the high bit.
    let t_bits = significand & ((1 << t) - 1);

    let max_exp = (1i32 << w) - 2;
    let bias: i32 = (1 << (w - 1)) - 1;
    exponent += bias + t as i32;

    if exponent > max_exp {
        Err("Magnitude too large")
    } else if exponent > 0 {
        // This is a normal number.
        let e_bits = (exponent as u64) << t;
        Ok(sign_bit | e_bits | t_bits)
    } else if 1 - exponent <= t as i32 {
        // This is a subnormal number: e = 0, t = significand bits.
        // Renormalize significand for exponent = 1.
        let adjust = 1 - exponent;
        if significand & ((1u64 << adjust) - 1) != 0 {
            Err("Subnormal underflow")
        } else {
            significand >>= adjust;
            Ok(sign_bit | significand)
        }
    } else {
        Err("Magnitude too small")
    }
}

impl Ieee32 {
    /// Create a new `Ieee32` representing the number `x`.
    pub fn new(x: f32) -> Ieee32 {
        Ieee32(x)
    }

    /// Construct `Ieee32` immediate from raw bits.
    pub fn from_bits(x: u32) -> Ieee32 {
        Ieee32(unsafe { mem::transmute(x) })
    }
}

impl Display for Ieee32 {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let bits: u32 = unsafe { mem::transmute(self.0) };
        format_float(bits as u64, 8, 23, f)
    }
}

impl FromStr for Ieee32 {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Ieee32, &'static str> {
        match parse_float(s, 8, 23) {
            Ok(b) => Ok(Ieee32::from_bits(b as u32)),
            Err(s) => Err(s),
        }
    }
}

impl Ieee64 {
    /// Create a new `Ieee64` representing the number `x`.
    pub fn new(x: f64) -> Ieee64 {
        Ieee64(x)
    }

    /// Construct `Ieee64` immediate from raw bits.
    pub fn from_bits(x: u64) -> Ieee64 {
        Ieee64(unsafe { mem::transmute(x) })
    }
}

impl Display for Ieee64 {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let bits: u64 = unsafe { mem::transmute(self.0) };
        format_float(bits, 11, 52, f)
    }
}

impl FromStr for Ieee64 {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Ieee64, &'static str> {
        match parse_float(s, 11, 52) {
            Ok(b) => Ok(Ieee64::from_bits(b)),
            Err(s) => Err(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{f32, f64};
    use std::str::FromStr;
    use std::fmt::Display;

    #[test]
    fn format_imm64() {
        assert_eq!(Imm64(0).to_string(), "0");
        assert_eq!(Imm64(9999).to_string(), "9999");
        assert_eq!(Imm64(10000).to_string(), "0x2710");
        assert_eq!(Imm64(-9999).to_string(), "-9999");
        assert_eq!(Imm64(-10000).to_string(), "0xffff_ffff_ffff_d8f0");
        assert_eq!(Imm64(0xffff).to_string(), "0xffff");
        assert_eq!(Imm64(0x10000).to_string(), "0x0001_0000");
    }

    // Verify that `text` can be parsed as a `T` into a value that displays as `want`.
    fn parse_ok<T: FromStr + Display>(text: &str, want: &str)
        where <T as FromStr>::Err: Display
    {
        match text.parse::<T>() {
            Err(s) => panic!("\"{}\".parse() error: {}", text, s),
            Ok(x) => assert_eq!(x.to_string(), want),
        }
    }

    // Verify that `text` fails to parse as `T` with the error `msg`.
    fn parse_err<T: FromStr + Display>(text: &str, msg: &str)
        where <T as FromStr>::Err: Display
    {
        match text.parse::<T>() {
            Err(s) => assert_eq!(s.to_string(), msg),
            Ok(x) => panic!("Wanted Err({}), but got {}", msg, x),
        }
    }

    #[test]
    fn parse_imm64() {
        parse_ok::<Imm64>("0", "0");
        parse_ok::<Imm64>("1", "1");
        parse_ok::<Imm64>("-0", "0");
        parse_ok::<Imm64>("-1", "-1");
        parse_ok::<Imm64>("0x0", "0");
        parse_ok::<Imm64>("0xf", "15");
        parse_ok::<Imm64>("-0x9", "-9");

        // Probe limits.
        parse_ok::<Imm64>("0xffffffff_ffffffff", "-1");
        parse_ok::<Imm64>("0x80000000_00000000", "0x8000_0000_0000_0000");
        parse_ok::<Imm64>("-0x80000000_00000000", "0x8000_0000_0000_0000");
        parse_err::<Imm64>("-0x80000000_00000001",
                           "Negative number too small for Imm64");
        parse_ok::<Imm64>("18446744073709551615", "-1");
        parse_ok::<Imm64>("-9223372036854775808", "0x8000_0000_0000_0000");
        // Overflow both the `checked_add` and `checked_mul`.
        parse_err::<Imm64>("18446744073709551616", "Too large decimal Imm64");
        parse_err::<Imm64>("184467440737095516100", "Too large decimal Imm64");
        parse_err::<Imm64>("-9223372036854775809",
                           "Negative number too small for Imm64");

        // Underscores are allowed where digits go.
        parse_ok::<Imm64>("0_0", "0");
        parse_ok::<Imm64>("-_10_0", "-100");
        parse_ok::<Imm64>("_10_", "10");
        parse_ok::<Imm64>("0x97_88_bb", "0x0097_88bb");
        parse_ok::<Imm64>("0x_97_", "151");

        parse_err::<Imm64>("", "No digits in Imm64");
        parse_err::<Imm64>("-", "No digits in Imm64");
        parse_err::<Imm64>("_", "No digits in Imm64");
        parse_err::<Imm64>("0x", "No digits in Imm64");
        parse_err::<Imm64>("0x_", "No digits in Imm64");
        parse_err::<Imm64>("-0x", "No digits in Imm64");
        parse_err::<Imm64>(" ", "Invalid character in decimal Imm64");
        parse_err::<Imm64>("0 ", "Invalid character in decimal Imm64");
        parse_err::<Imm64>(" 0", "Invalid character in decimal Imm64");
        parse_err::<Imm64>("--", "Invalid character in decimal Imm64");
        parse_err::<Imm64>("-0x-", "Invalid character in hexadecimal Imm64");

        // Hex count overflow.
        parse_err::<Imm64>("0x0_0000_0000_0000_0000",
                           "Too many hexadecimal digits in Imm64");
    }

    #[test]
    fn format_ieee32() {
        assert_eq!(Ieee32::new(0.0).to_string(), "0.0");
        assert_eq!(Ieee32::new(-0.0).to_string(), "-0.0");
        assert_eq!(Ieee32::new(1.0).to_string(), "0x1.000000p0");
        assert_eq!(Ieee32::new(1.5).to_string(), "0x1.800000p0");
        assert_eq!(Ieee32::new(0.5).to_string(), "0x1.000000p-1");
        assert_eq!(Ieee32::new(f32::EPSILON).to_string(), "0x1.000000p-23");
        assert_eq!(Ieee32::new(f32::MIN).to_string(), "-0x1.fffffep127");
        assert_eq!(Ieee32::new(f32::MAX).to_string(), "0x1.fffffep127");
        // Smallest positive normal number.
        assert_eq!(Ieee32::new(f32::MIN_POSITIVE).to_string(),
                   "0x1.000000p-126");
        // Subnormals.
        assert_eq!(Ieee32::new(f32::MIN_POSITIVE / 2.0).to_string(),
                   "0x0.800000p-126");
        assert_eq!(Ieee32::new(f32::MIN_POSITIVE * f32::EPSILON).to_string(),
                   "0x0.000002p-126");
        assert_eq!(Ieee32::new(f32::INFINITY).to_string(), "Inf");
        assert_eq!(Ieee32::new(f32::NEG_INFINITY).to_string(), "-Inf");
        assert_eq!(Ieee32::new(f32::NAN).to_string(), "NaN");
        assert_eq!(Ieee32::new(-f32::NAN).to_string(), "-NaN");
        // Construct some qNaNs with payloads.
        assert_eq!(Ieee32::from_bits(0x7fc00001).to_string(), "NaN:0x1");
        assert_eq!(Ieee32::from_bits(0x7ff00001).to_string(), "NaN:0x300001");
        // Signaling NaNs.
        assert_eq!(Ieee32::from_bits(0x7f800001).to_string(), "sNaN:0x1");
        assert_eq!(Ieee32::from_bits(0x7fa00001).to_string(), "sNaN:0x200001");
    }

    #[test]
    fn parse_ieee32() {
        parse_ok::<Ieee32>("0.0", "0.0");
        parse_ok::<Ieee32>("-0.0", "-0.0");
        parse_ok::<Ieee32>("0x0", "0.0");
        parse_ok::<Ieee32>("0x0.0", "0.0");
        parse_ok::<Ieee32>("0x.0", "0.0");
        parse_ok::<Ieee32>("0x0.", "0.0");
        parse_ok::<Ieee32>("0x1", "0x1.000000p0");
        parse_ok::<Ieee32>("-0x1", "-0x1.000000p0");
        parse_ok::<Ieee32>("0x10", "0x1.000000p4");
        parse_ok::<Ieee32>("0x10.0", "0x1.000000p4");
        parse_err::<Ieee32>("0.", "Float must be hexadecimal");
        parse_err::<Ieee32>(".0", "Float must be hexadecimal");
        parse_err::<Ieee32>("0", "Float must be hexadecimal");
        parse_err::<Ieee32>("-0", "Float must be hexadecimal");
        parse_err::<Ieee32>(".", "Float must be hexadecimal");
        parse_err::<Ieee32>("", "Float must be hexadecimal");
        parse_err::<Ieee32>("-", "Float must be hexadecimal");
        parse_err::<Ieee32>("0x", "No digits");
        parse_err::<Ieee32>("0x..", "Multiple radix points");

        // Check significant bits.
        parse_ok::<Ieee32>("0x0.ffffff", "0x1.fffffep-1");
        parse_ok::<Ieee32>("0x1.fffffe", "0x1.fffffep0");
        parse_ok::<Ieee32>("0x3.fffffc", "0x1.fffffep1");
        parse_ok::<Ieee32>("0x7.fffff8", "0x1.fffffep2");
        parse_ok::<Ieee32>("0xf.fffff0", "0x1.fffffep3");
        parse_err::<Ieee32>("0x1.ffffff", "Too many significant bits");
        parse_err::<Ieee32>("0x1.fffffe0000000000", "Too many digits");

        // Exponents.
        parse_ok::<Ieee32>("0x1p3", "0x1.000000p3");
        parse_ok::<Ieee32>("0x1p-3", "0x1.000000p-3");
        parse_ok::<Ieee32>("0x1.0p3", "0x1.000000p3");
        parse_ok::<Ieee32>("0x2.0p3", "0x1.000000p4");
        parse_ok::<Ieee32>("0x1.0p127", "0x1.000000p127");
        parse_ok::<Ieee32>("0x1.0p-126", "0x1.000000p-126");
        parse_ok::<Ieee32>("0x0.1p-122", "0x1.000000p-126");
        parse_err::<Ieee32>("0x2.0p127", "Magnitude too large");

        // Subnormals.
        parse_ok::<Ieee32>("0x1.0p-127", "0x0.800000p-126");
        parse_ok::<Ieee32>("0x1.0p-149", "0x0.000002p-126");
        parse_ok::<Ieee32>("0x0.000002p-126", "0x0.000002p-126");
        parse_err::<Ieee32>("0x0.100001p-126", "Subnormal underflow");
        parse_err::<Ieee32>("0x1.8p-149", "Subnormal underflow");
        parse_err::<Ieee32>("0x1.0p-150", "Magnitude too small");

        // NaNs and Infs.
        parse_ok::<Ieee32>("Inf", "Inf");
        parse_ok::<Ieee32>("-Inf", "-Inf");
        parse_ok::<Ieee32>("NaN", "NaN");
        parse_ok::<Ieee32>("-NaN", "-NaN");
        parse_ok::<Ieee32>("NaN:0x0", "NaN");
        parse_err::<Ieee32>("NaN:", "Float must be hexadecimal");
        parse_err::<Ieee32>("NaN:0", "Float must be hexadecimal");
        parse_err::<Ieee32>("NaN:0x", "Invalid NaN payload");
        parse_ok::<Ieee32>("NaN:0x000001", "NaN:0x1");
        parse_ok::<Ieee32>("NaN:0x300001", "NaN:0x300001");
        parse_err::<Ieee32>("NaN:0x400001", "Invalid NaN payload");
        parse_ok::<Ieee32>("sNaN:0x1", "sNaN:0x1");
        parse_err::<Ieee32>("sNaN:0x0", "Invalid sNaN payload");
        parse_ok::<Ieee32>("sNaN:0x200001", "sNaN:0x200001");
        parse_err::<Ieee32>("sNaN:0x400001", "Invalid sNaN payload");
    }

    #[test]
    fn format_ieee64() {
        assert_eq!(Ieee64::new(0.0).to_string(), "0.0");
        assert_eq!(Ieee64::new(-0.0).to_string(), "-0.0");
        assert_eq!(Ieee64::new(1.0).to_string(), "0x1.0000000000000p0");
        assert_eq!(Ieee64::new(1.5).to_string(), "0x1.8000000000000p0");
        assert_eq!(Ieee64::new(0.5).to_string(), "0x1.0000000000000p-1");
        assert_eq!(Ieee64::new(f64::EPSILON).to_string(),
                   "0x1.0000000000000p-52");
        assert_eq!(Ieee64::new(f64::MIN).to_string(), "-0x1.fffffffffffffp1023");
        assert_eq!(Ieee64::new(f64::MAX).to_string(), "0x1.fffffffffffffp1023");
        // Smallest positive normal number.
        assert_eq!(Ieee64::new(f64::MIN_POSITIVE).to_string(),
                   "0x1.0000000000000p-1022");
        // Subnormals.
        assert_eq!(Ieee64::new(f64::MIN_POSITIVE / 2.0).to_string(),
                   "0x0.8000000000000p-1022");
        assert_eq!(Ieee64::new(f64::MIN_POSITIVE * f64::EPSILON).to_string(),
                   "0x0.0000000000001p-1022");
        assert_eq!(Ieee64::new(f64::INFINITY).to_string(), "Inf");
        assert_eq!(Ieee64::new(f64::NEG_INFINITY).to_string(), "-Inf");
        assert_eq!(Ieee64::new(f64::NAN).to_string(), "NaN");
        assert_eq!(Ieee64::new(-f64::NAN).to_string(), "-NaN");
        // Construct some qNaNs with payloads.
        assert_eq!(Ieee64::from_bits(0x7ff8000000000001).to_string(), "NaN:0x1");
        assert_eq!(Ieee64::from_bits(0x7ffc000000000001).to_string(),
                   "NaN:0x4000000000001");
        // Signaling NaNs.
        assert_eq!(Ieee64::from_bits(0x7ff0000000000001).to_string(),
                   "sNaN:0x1");
        assert_eq!(Ieee64::from_bits(0x7ff4000000000001).to_string(),
                   "sNaN:0x4000000000001");
    }

    #[test]
    fn parse_ieee64() {
        parse_ok::<Ieee64>("0.0", "0.0");
        parse_ok::<Ieee64>("-0.0", "-0.0");
        parse_ok::<Ieee64>("0x0", "0.0");
        parse_ok::<Ieee64>("0x0.0", "0.0");
        parse_ok::<Ieee64>("0x.0", "0.0");
        parse_ok::<Ieee64>("0x0.", "0.0");
        parse_ok::<Ieee64>("0x1", "0x1.0000000000000p0");
        parse_ok::<Ieee64>("-0x1", "-0x1.0000000000000p0");
        parse_ok::<Ieee64>("0x10", "0x1.0000000000000p4");
        parse_ok::<Ieee64>("0x10.0", "0x1.0000000000000p4");
        parse_err::<Ieee64>("0.", "Float must be hexadecimal");
        parse_err::<Ieee64>(".0", "Float must be hexadecimal");
        parse_err::<Ieee64>("0", "Float must be hexadecimal");
        parse_err::<Ieee64>("-0", "Float must be hexadecimal");
        parse_err::<Ieee64>(".", "Float must be hexadecimal");
        parse_err::<Ieee64>("", "Float must be hexadecimal");
        parse_err::<Ieee64>("-", "Float must be hexadecimal");
        parse_err::<Ieee64>("0x", "No digits");
        parse_err::<Ieee64>("0x..", "Multiple radix points");

        // Check significant bits.
        parse_ok::<Ieee64>("0x0.fffffffffffff8", "0x1.fffffffffffffp-1");
        parse_ok::<Ieee64>("0x1.fffffffffffff", "0x1.fffffffffffffp0");
        parse_ok::<Ieee64>("0x3.ffffffffffffe", "0x1.fffffffffffffp1");
        parse_ok::<Ieee64>("0x7.ffffffffffffc", "0x1.fffffffffffffp2");
        parse_ok::<Ieee64>("0xf.ffffffffffff8", "0x1.fffffffffffffp3");
        parse_err::<Ieee64>("0x3.fffffffffffff", "Too many significant bits");
        parse_err::<Ieee64>("0x001.fffffe00000000", "Too many digits");

        // Exponents.
        parse_ok::<Ieee64>("0x1p3", "0x1.0000000000000p3");
        parse_ok::<Ieee64>("0x1p-3", "0x1.0000000000000p-3");
        parse_ok::<Ieee64>("0x1.0p3", "0x1.0000000000000p3");
        parse_ok::<Ieee64>("0x2.0p3", "0x1.0000000000000p4");
        parse_ok::<Ieee64>("0x1.0p1023", "0x1.0000000000000p1023");
        parse_ok::<Ieee64>("0x1.0p-1022", "0x1.0000000000000p-1022");
        parse_ok::<Ieee64>("0x0.1p-1018", "0x1.0000000000000p-1022");
        parse_err::<Ieee64>("0x2.0p1023", "Magnitude too large");

        // Subnormals.
        parse_ok::<Ieee64>("0x1.0p-1023", "0x0.8000000000000p-1022");
        parse_ok::<Ieee64>("0x1.0p-1074", "0x0.0000000000001p-1022");
        parse_ok::<Ieee64>("0x0.0000000000001p-1022", "0x0.0000000000001p-1022");
        parse_err::<Ieee64>("0x0.10000000000008p-1022", "Subnormal underflow");
        parse_err::<Ieee64>("0x1.8p-1074", "Subnormal underflow");
        parse_err::<Ieee64>("0x1.0p-1075", "Magnitude too small");

        // NaNs and Infs.
        parse_ok::<Ieee64>("Inf", "Inf");
        parse_ok::<Ieee64>("-Inf", "-Inf");
        parse_ok::<Ieee64>("NaN", "NaN");
        parse_ok::<Ieee64>("-NaN", "-NaN");
        parse_ok::<Ieee64>("NaN:0x0", "NaN");
        parse_err::<Ieee64>("NaN:", "Float must be hexadecimal");
        parse_err::<Ieee64>("NaN:0", "Float must be hexadecimal");
        parse_err::<Ieee64>("NaN:0x", "Invalid NaN payload");
        parse_ok::<Ieee64>("NaN:0x000001", "NaN:0x1");
        parse_ok::<Ieee64>("NaN:0x4000000000001", "NaN:0x4000000000001");
        parse_err::<Ieee64>("NaN:0x8000000000001", "Invalid NaN payload");
        parse_ok::<Ieee64>("sNaN:0x1", "sNaN:0x1");
        parse_err::<Ieee64>("sNaN:0x0", "Invalid sNaN payload");
        parse_ok::<Ieee64>("sNaN:0x4000000000001", "sNaN:0x4000000000001");
        parse_err::<Ieee64>("sNaN:0x8000000000001", "Invalid sNaN payload");
    }
}
