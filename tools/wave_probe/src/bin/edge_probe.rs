//! Per-word trace of a resettable counter: measures exactly how many fabric
//! clock edges the VeriComm fixture delivers per tx word.
//!
//! Stream: word0 = rst(0x0008), words 1..N = 0x0000, rest = 0x0000.
//! rx gives one sampled nibble per word slot -> watch cnt evolve.

use anyhow::Result;
use vlfd_rs::{Device, IoSettings};

fn main() -> Result<()> {
    let _bitstream = std::env::args()
        .nth(1)
        .expect("usage: edge_probe <bitstream> [words]");
    let n_words: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);

    let mut device = Device::connect()?;
    println!(
        "device programmed={} fifo={} vericomm={} clock_continues={}",
        device.config().is_programmed(),
        device.config().fifo_size(),
        device.config().vericomm_ability(),
        device.config().vericomm_clock_continues()
    );
    device.enter_io_mode(&IoSettings::default())?;

    let mut tx = vec![0u16; 1024];
    let mut rx = vec![0u16; 1024];
    tx[0] = 0x0008; // rst high for first word only
    // words 1..n_words: rst low (counter counts), rest stay zero (still count!)

    device.transfer_io(&mut tx, &mut rx)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    println!("word | raw    | cnt(nibble)");
    for (index, word) in rx.iter().enumerate().take(n_words) {
        let nibble = word & 0xf;
        println!(
            "{index:4} | {word:04x}   | {nibble:x} {}",
            "#".repeat(nibble as usize)
        );
    }

    device.exit_io_mode()?;
    Ok(())
}
