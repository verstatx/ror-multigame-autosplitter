# Risk of Rain series multigame autosplitter

A cross-platform multigame autosplitter for the Risk of Rain series.
For use with LiveSplit and LiveSplit One.
Currently supported games:

* Risk of Rain
  * Only version 1.2.2
* Risk of Rain 2
  * All versions (as of SotS 1.3.6#171)
* Risk of Rain: Returns
  * v1.0.3-v1.0.5

## Usage

Download the latest `ror_multigame_autosplitter.wasm` from the release section. In LiveSplit, configure the "autosplitting runtime" component to use the downloaded .wasm file. Configure the autosplitter settings once it loads. Do not forget to change the comparison method from "Real Time" to  "Game Time".

LiveSplit One is also supported, see the project specific documentation on how to configure autosplitting.

## Building

Make sure you have the wasm32 target installed:
```sh
rustup target add wasm32-unknown-unknown
```

Then build using:
```sh
cargo build --release
```

The compiled output .wasm will be located in `target/wasm32-unknown-unknown/release/ror_multigame_autosplitter.wasm`.

## Known Issues and Limitations
* Rust
* The first split has a slightly (<1ms) lower "Game Time" than "Real Time".  
  This is due to a workaround for LiveSplit currently having no way for an autosplitter to initialize "Game Time".  
  Without the workaround, LiveSplit will not show the split time for the splits before Game Time has been modified (game swap, or the end of Risk of Rain 2's stage 1).
* Incomplete version support for Risk of Rain

