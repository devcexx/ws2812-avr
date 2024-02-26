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
use core::arch::asm;

// This code requires Rust Nightly 2022-08-12 to compile because of
// https://github.com/rust-lang/rust/issues/84669
pub trait IsTrue {}
pub struct BExpr<const B: bool> {}
impl IsTrue for BExpr<true> {}

pub trait NopGen {
    fn gen();
}

pub struct NopBlock<const SIZE: u8> {}

impl NopGen for NopBlock<0> {
    #[inline(always)]
    fn gen() {}
}

impl NopGen for NopBlock<1> {
    #[inline(always)]
    fn gen() {
        unsafe {
            asm!("nop");
        }
    }
}

impl<const N: u8> NopGen for NopBlock<N>
where
    BExpr<{ N > 1 }>: IsTrue,
    NopBlock<{ N - 2 }>: NopGen,
{
    #[inline(always)]
    fn gen() {
        unsafe {
            // This is an AVR instruction that is equivalent to doing nothing
            // (relative jump to +0, which basically means doing nothing useful)
            // but takes 2 cycles to complete. We can use it to save a
            // few bytes by collapsing two NOP cycles into a single
            // instruction.
            asm!("rjmp +0");
        }

        <NopBlock<{ N - 2 }> as NopGen>::gen();
    }
}
