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
#![no_std]
#![no_main]
#![allow(incomplete_features)] // I live on the limit, like a derivative.
#![feature(asm_experimental_arch)]
#![feature(asm_const)]
#![feature(generic_const_exprs)]
#![feature(never_type)]
#![feature(const_mut_refs)]
#![feature(adt_const_params)]
#![feature(const_trait_impl)]
#![feature(const_slice_index)]
#![feature(effects)]

mod color;
mod ports;
pub mod util;
pub use color::*;

use crate::util::asm::{asm_block, branch_not_equal, dec, ld_immediate, lsl, skip_if_bit_set};
use arduino_hal::port::{mode::Output, Pin};
use avr_hal_generic::avr_device::interrupt::free;
use core::marker::PhantomData;
use core::mem::size_of;
use ports::{StaticPin, StaticPort};
use util::time::TimeVal;
use util::{time, NopBlock, NopGen};

mod consts {
    use arduino_hal::{clock::Clock, DefaultClock};

    /** The number of fixed cycles (non-nop instructions) consumed by
    the instructions of the loop after enabling the pin until it is
    disabled, when the value of the bit that is being set is zero. The
    current value is based on the behavior of this code, which is in
    charge of enabling the pin during data sending:
    ```no_run
    ...
    pin_high();                      // Pin is enabled just after this instruction, so this cycle is not counted.
    Ts::S1Nops::gen();               // Generates a variable number of NOP instructions, not counted.
    skip_if_bit_set!(curbyte, "7");  // sbrs instruction. Takes 2 cycles when bit is set, 1 when not. Therefore, it counts as 1.
    pin_low();                       // Pin is disabled just after this instruction, so it counts as 1.
    ...
    ```
     */
    pub const FIXED_CYCLES_T0H: u8 = 2;

    /**
    The number of fixed cycles (non-nop instructions) consumed by the
    instructions of the loop after enabling the pin until it is
    disabled, when the value of the bit that is being set is one. The
    current value is based on the behavior of this code, which is in
    charge of enabling the pin during data sending:
    ```no_run
    ...
    pin_high();                      // Pin is enabled just after this instruction, so this cycle is not counted.
    Ts::S1Nops::gen();               // Generates a variable number of NOP instructions, not counted.
    skip_if_bit_set!(curbyte, "7");  // sbrs instruction. Takes 2 cycles when bit is set, 1 when not. Therefore, it counts as 2.
    pin_low();                       // Instruction skipped by previous instruction.
    lsl!(curbyte);                   // lsl instruction, takes 1 cycle.
    Ts::S2Nops::gen();               // Generates a variable number of NOP instructions, not counted.
    pin_low();                       // Pin is disabled just after this instruction, so it counts as 1.
    ...
    ```
    */
    pub const FIXED_CYCLES_T1H: u8 = 4;

    /**
    The number of fixed cycles (non-nop instructions) that takes the
    program to give a full loop through the code in charge of send
    color signals to the device. The current value for this constant
    is calculated by counting the fixed instructions from the code in
    charge of sending this data to the device:
    ```no_run
    ...
    2: pin_high();                   // When the code loops, it ends here putting the pin high again. So it counts as 1 cycle.
    Ts::S1Nops::gen();               // Generates a variable number of NOP instructions, not counted.
    skip_if_bit_set!(curbyte, "7");  // This and the next instruction
                                     // always count as two cycles: if the next instruction is skipped,
                                     // then the sbrs instruction takes 2 cycles. If it is not skipped,
                                     // sbrs takes 1 cycle and the next one takes 1 cycle as well.
    pin_low();
    lsl!(curbyte);                   // Takes 1 instruction.
    Ts::S2Nops::gen();               // Generates a variable number of NOP instructions, not counted.
    pin_low();                       // Takes 1 instruction.
    Ts::S3Nops::gen();               // Generates a variable number of NOP instructions, not counted.
    dec!(i);                         // Takes 1 instruction.
    branch_not_equal!("2b");         // Takes 2 instructions when the loop continues.
    ...
    ```
    */
    pub const FIXED_CYCLES_TOTAL: u8 = 8;

    /// Holds the clock speed of the CPU
    pub const F_CPU: u32 = DefaultClock::FREQ;
    pub(crate) const NANOS_IN_SECOND: u64 = 1000000000;
}

macro_rules! diff_clamp_zero {
    ($a:expr, $b: expr) => {
	if ($a) >= ($b) {
	    ($a) - ($b)
	} else {
	    0
	}
    };

    ($a:expr, $b: expr, $($c:expr),*) => {
	if ($a) >= ($b) {
	    diff_clamp_zero!(($a) - ($b), $($c),*)
	} else {
	    0
	}
    };
}

// Seems that Rust and LLVM will remove duplicated functions with the
// same code, so no matter whether there's duplicated impls that they
// will not generate extra code. https://github.com/rust-lang/rust/issues/46477

/**
 * Allows to define the precise timings that are going to be used
 * while sending data to the device.
 */
pub trait Timings {
    /// The reset time that the MCU must sleep to make sure the device
    /// understands the data sending has terminated.
    type Rst: time::TimeVal;

    /// The total time it takes to send a color bit to the device.
    type Cycle: time::TimeVal;

    /// The time that the signal in the pin should be kept at high for sending a bit with value 1.
    type T1h: time::TimeVal;

    /// The time that the signal in the pin should be kept at high for sending a bit with value 0.
    type T0h: time::TimeVal;
}

/**
 * The default timings for sending data to a WS2812 device. It uses a
 * conservative cycle timing of 1.25 us and a reset time of 250 us,
 * that should work with the majority of the WS2812 devices.
 *
 * When defining custom timings, user must comply with some invariants
 * that are currently not being checked by the compiler:
 *
 * - T1h + T0h must be less than Cycle.
 * - T0h must be less than T1h.
 */
pub struct DefaultTimings {}

impl Timings for DefaultTimings {
    type Rst = time::Time<time::Micros, 250>;
    type Cycle = time::Time<time::Nanos, 1250>;
    type T1h = time::Time<time::Nanos, 900>;
    type T0h = time::Time<time::Nanos, 350>;
}

/**
 * Defines generic types for the NOP blocks generation based on the
 * values from the [Timings] and [CalculatedTimings] traits.
*/
pub trait TypedTimings: Timings {
    type S1Nops: NopGen;
    type S2Nops: NopGen;
    type S3Nops: NopGen;
}

/**
 * Defines calculations constants derivated from the user defined timings.
 */
pub trait CalculatedTimings {
    /// Total number of cycles required the T0H signal to be enabled.
    const T0H_CYCLES: u8;
    /// Total number of cycles required the T1H signal to be enabled.
    const T1H_CYCLES: u8;

    /// Total number of CPU cycles that it takes to run a full cycle
    /// of enabling and disabling the pin.
    const TOTAL_CYCLES: u8;

    /// Number of NOP cycles at stage 1.
    const S1_NOPS: u8;
    /// Number of NOP cycles at stage 2.
    const S2_NOPS: u8;
    /// Number of NOP cycles at stage 3.
    const S3_NOPS: u8;
}

impl<Ts: Timings> CalculatedTimings for Ts {
    const T0H_CYCLES: u8 = (consts::F_CPU as u64 * Ts::T0h::NANOS / consts::NANOS_IN_SECOND) as u8;
    const T1H_CYCLES: u8 = (consts::F_CPU as u64 * Ts::T1h::NANOS / consts::NANOS_IN_SECOND) as u8;
    const TOTAL_CYCLES: u8 =
        (consts::F_CPU as u64 * Ts::Cycle::NANOS / consts::NANOS_IN_SECOND) as u8;

    const S1_NOPS: u8 = diff_clamp_zero!(Self::T0H_CYCLES, consts::FIXED_CYCLES_T0H);
    const S2_NOPS: u8 = diff_clamp_zero!(Self::T1H_CYCLES, consts::FIXED_CYCLES_T1H, Self::S1_NOPS);
    const S3_NOPS: u8 = diff_clamp_zero!(
        Self::TOTAL_CYCLES,
        consts::FIXED_CYCLES_TOTAL,
        Self::S1_NOPS,
        Self::S2_NOPS
    );
}

impl<Ts: Timings> TypedTimings for Ts
where
    NopBlock<{ Ts::S1_NOPS }>: NopGen,
    NopBlock<{ Ts::S2_NOPS }>: NopGen,
    NopBlock<{ Ts::S3_NOPS }>: NopGen,
{
    type S1Nops = NopBlock<{ Ts::S1_NOPS }>;
    type S2Nops = NopBlock<{ Ts::S2_NOPS }>;
    type S3Nops = NopBlock<{ Ts::S3_NOPS }>;
}

/**
 * Represents a driver for WS2812 leds.
 */
#[repr(transparent)]
pub struct WS2812<P, Ts, Order> {
    _pin: Pin<Output, P>,
    _ts: PhantomData<Ts>,
    _order: PhantomData<Order>,
}

/**
 * The WS2812 driver type that uses the default timings defined at [DefaultTimings].
 */
type WS2812Default<Pin, Order> = WS2812<Pin, DefaultTimings, Order>;

impl WS2812<!, !, !> {
    pub fn new<P: StaticPin, Order>(pin: Pin<Output, P>) -> WS2812Default<P, Order> {
        WS2812 {
            _pin: pin,
            _ts: PhantomData,
            _order: PhantomData,
        }
    }

    pub fn new_with_timings<Ts, P: StaticPin, Order>(pin: Pin<Output, P>) -> WS2812<P, Ts, Order> {
        WS2812 {
            _pin: pin,
            _ts: PhantomData,
            _order: PhantomData,
        }
    }
}

impl<Pin: StaticPin, Ts: TypedTimings, Order: ColorOrder> WS2812<Pin, Ts, Order> {
    pub fn write(&mut self, data: &[Order]) {
        free(|_cs| {
            // SAFETY:
            // - Pin ownership is ensured by holding it into this structure.
            // - Previous call to free ensures a interrupt-free context.
            unsafe {
                let port_value: u8 = Pin::Port::read();
                let maskhi = port_value | (1 << Pin::PIN_BIT_INDEX);
                let masklo = port_value & !(1 << Pin::PIN_BIT_INDEX);
                ws2812_write::<Pin::Port, Ts>(
                    data.as_ptr() as *const u8,
                    data.len() * size_of::<Order>(),
                    maskhi,
                    masklo,
                );
            }
        });

        arduino_hal::delay_us(Ts::Rst::MICROS as u32);
    }
}

/// Perform a raw write into a WS2812 device of the given data.
#[allow(unused_assignments)]
pub unsafe fn ws2812_write<P: StaticPort, Ts: TypedTimings>(
    mut data: *const u8,
    mut len: usize,
    maskhi: u8,
    masklo: u8,
) {
    let mut i: u8;
    let mut curbyte: u8;

    let pin_low = || P::write(masklo);
    let pin_high = || P::write(maskhi);

    while len > 0 {
        curbyte = *data;

        ld_immediate!(i, "8");
        asm_block!("2", {
            // Loop for iterate on each bit of each byte of the input
            // Initially, regardless of the value of the bit (either 0
            // or 1), the pin is always enabled. After summing the
            // fixed cycles and the S1 Nops, the pin will be kept
            // enabled for at least T0h ns.
            pin_high();
            Ts::S1Nops::gen();

            // If the bit value is 0, then the pin_low() is executed
            // and the pin is kept low for (Cycle - T0h) nanos, until
            // the next iteration of the loop. If the bit is 1, and
            // T1h is always greater to T0h, then the pin_low() is not
            // executed and the pin is kept high for (T1h - T0h)
            // nanos.
            skip_if_bit_set!(curbyte, "7");
            pin_low();
            lsl!(curbyte);
            Ts::S2Nops::gen();

            // If the bit is zero, then it does nothing because the
            // pin is already low. If the bit is 1, it turns off the
            // pin is kept low until the next iteration.
            pin_low();
            Ts::S3Nops::gen();
            dec!(i);
            branch_not_equal!("2b");
        });

        data = data.add(1);
        len -= 1;
    }
}
