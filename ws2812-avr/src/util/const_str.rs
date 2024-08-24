/*
This file is part of ws2812-avr.

ws2812-avr is free software: you can redistribute it and/or modify it
under the terms of the GNU General Public License as published by the
Free Software Foundation, either version 3 of the License, or (at your
option) any later version.

ws2812-avr is distributed in the hope that it will be useful, but
WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
General Public License for more details.

You should have received a copy of the GNU General Public License
along with ws2812-avr. If not, see <https://www.gnu.org/licenses/>.
 */

/*! Contains utilities for working with compile-time constant strings,
 * allocated in stack arrays of a size known at compile time. It is
 * currently used for construct diagnostic strings that can be used to
 * panic in const evaluation, for showing informative messages to the
 * user. */
#[const_trait]
trait CopyFromSliceConst<A> {
    fn const_copy_from_slice(&mut self, other: &[A]);
}

impl<A: Copy> const CopyFromSliceConst<A> for [A] {
    fn const_copy_from_slice(&mut self, other: &[A]) {
        let mut i = 0;
        while i < self.len() {
            self[i] = other[i];
            i += 1;
        }
    }
}

/**
Represents an argument from the [const_concat2] function whose value is known at compile time.
*/
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ConstArg<const LEN: usize> {
    String(ConstStr<LEN>),
    Integer(NumLengthed<LEN>),
}

impl<const LEN: usize> ConstArg<LEN> {
    /**
    Converts the current enum variant into its string representation.
    */
    pub const fn into_const_str(self) -> ConstStr<LEN> {
        match self {
            ConstArg::String(s) => s,
            ConstArg::Integer(i) => num_to_string(i),
        }
    }
}

/**
Identifies a string allocated in stack where its size is known in
compile time. When a [ConstStr] is instantiated, the bytes held by
this instance is guaranteed to hold a valid UTF-8 string.
*/
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct ConstStr<const LEN: usize> {
    data: [u8; LEN],
}

// impl<const LEN: usize> const AsRef<str> for ConstStr<LEN> {
//     fn as_ref(&self) -> &str {
//         unsafe { core::str::from_utf8_unchecked(&self.data) }
//     }
// }

impl ConstStr<0> {
    /**
    Builds a [ConstStr] from a &'static str string.
    */
    pub const fn from_static<const S: &'static str>() -> ConstStr<{ S.len() }> {
        let mut buf = [0u8; S.len()];
        buf.const_copy_from_slice(&S.as_bytes());
        ConstStr { data: buf }
    }
}

impl<const LEN: usize> ConstStr<LEN> {
    /**
    Constructs a [ConstStr] from a raw buffer. This operation is
    unsafe because the caller must make sure the input buffer is a
    valid UTF-8 buffer.
    */
    pub const unsafe fn from_raw_parts(buf: [u8; LEN]) -> ConstStr<LEN> {
        ConstStr { data: buf }
    }

    /**
    Returns the array slice that contains the data represented by the string.
    */
    pub const fn as_bytes(&self) -> &[u8; LEN] {
        &self.data
    }
}

/**
Represents a i128 number whose string representation length is known at compile time.
*/
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct NumLengthed<const LEN: usize> {
    value: i128,
}

impl NumLengthed<0> {
    /**
    Instantiates a [NumLengthed] from a const number.
    */
    pub const fn new<const N: i128>() -> NumLengthed<{ calc_num_len(N) }> {
        NumLengthed { value: N }
    }

    /**
    Gets the number held by this struct.
    */
    pub const fn value(&self) -> i128 {
        self.value
    }
}

/**
Calculates the length of a number when represented as a string.
*/
pub const fn calc_num_len(mut num: i128) -> usize {
    let mut len = 0;
    if num < 0 {
        // Negative sign
        len += 1;
    }

    while {
        num /= 10;

        len += 1;
        num > 0
    } {}

    len
}

/**
Converts a integer number to its string representation.
*/
pub const fn num_to_string<const LEN: usize>(value: NumLengthed<LEN>) -> ConstStr<LEN> {
    let mut num = value.value;
    let mut buf = [0u8; LEN];
    if num < 0 {
        buf[0] = '-' as u8;
        num = -num;
    }

    let mut offset = buf.len();
    while {
        let rem = num % 10;
        num /= 10;

        offset -= 1;
        buf[offset] = '0' as u8 + (rem as u8);
        num > 0
    } {}

    ConstStr { data: buf }
}

/**
Concats two constant arguments into a [ConstStr].
*/
pub const fn const_concat2<const LN: usize, const RN: usize>(
    l: ConstArg<LN>,
    r: ConstArg<RN>,
) -> ConstStr<{ LN + RN }> {
    let mut buf = [0u8; LN + RN];
    let lbuf = l.into_const_str();
    let rbuf = r.into_const_str();
    buf[0..LN].const_copy_from_slice(&lbuf.data);
    buf[LN..].const_copy_from_slice(&rbuf.data);

    unsafe { ConstStr::from_raw_parts(buf) }
}

#[macro_export]
macro_rules! const_concat {
    (@component s($value:expr)) => {
	$crate::util::const_str::ConstArg::String($crate::util::const_str::ConstStr::from_static::<{$value}>())
    };

    (@component cs($value:expr)) => {
	$crate::util::const_str::ConstArg::String($value)
    };

    (@component d($value:expr)) => {
	$crate::util::const_str::ConstArg::Integer($crate::util::const_str::NumLengthed::new::<{$value as i128}>())
    };

    (@component v($value:expr)) => {
	$value
    };

    () => {
	$crate::util::const_str::ConstStr::from_static::<"">()
    };

    ($format_spec:ident($component:expr)) => {
	$crate::util::const_str::const_concat!(@component $format_spec($component)).into_const_str()
    };

    ($format_spec1:ident($component1:expr), $format_spec2:ident($component2:expr)) => {
	$crate::util::const_str::const_concat2(
	    $crate::util::const_str::const_concat!(@component $format_spec1($component1)),
	    $crate::util::const_str::const_concat!(@component $format_spec2($component2))
	)
    };

    ($format_spec1:ident($component1:expr), $format_spec2:ident($component2:expr), $($format_spec:ident($component:expr)),+) => {
	$crate::util::const_str::const_concat2(
	    $crate::util::const_str::ConstArg::String($crate::util::const_str::const_concat2(
		$crate::util::const_str::const_concat!(@component $format_spec1($component1)),
		$crate::util::const_str::const_concat!(@component $format_spec2($component2))
	    )),
	    $crate::util::const_str::ConstArg::String($crate::util::const_str::const_concat!(
		$($format_spec($component)),*
	    ))
	)
    };
}
