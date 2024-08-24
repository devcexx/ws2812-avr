# ws2812-avr

WS2812 clockless led strip driver for AVR devices developed in pure
Rust.

This project was done for fun as a part of a Rust learning process. It
uses extremely unstable features from the nigthly compiler, such as
`const_generic_exprs` because it is more entertaining and useful for
me to using them than not doing so. This library may be used as long
as you keep that in mind.

Tested on Rust nightly 2024-04-15 (Minimum version required).

## Usage

1. Follow instructions [here](https://github.com/Rahix/avr-hal) to
   setup a Rust AVR project using avr-hal crates.
   
2. Add it to your Cargo.toml as follows:
   ```
   [dependencies]
   ...
   ws2812-avr = { git = "https://github.com/devcexx/ws2812-avr", rev = "<commit id>", features = ["<avr-hal processor name>"] }
   ```

3. Add the feature `#![feature(generic_const_exprs)]` to your main
   Rust file.
   
Check the [examples](examples) folder for checking some examples on
how to use the library. Review the library docstrings for getting
documentation about the components of the library.
