//! Per-sample trace of a resettable counter: demonstrates that VeriComm
//! captures an asynchronously free-running fabric rather than clock steps.
//!
//! Stream: sample 0 = rst(0x0008), later samples = 0x0000.

use anyhow::{Result, bail};
use vlfd_rs::{Board, BoardSelector, IoConfig, Licence, Programmer, VeriCommFrame};
use wave_probe::profile::BoardProfile;

fn main() -> Result<()> {
    let bitstream = std::env::args()
        .nth(1)
        .expect("usage: edge_probe <bitstream> [words]");
    let n_samples: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);

    let profile = BoardProfile::bundled()?;
    let boards = Board::enumerate()?;
    let [board_info] = boards.as_slice() else {
        bail!(
            "edge_probe requires exactly one connected board, found {}",
            boards.len()
        );
    };
    let selector = BoardSelector::UsbLocation(board_info.location.clone());
    let mut programmer = Programmer::open_selected(&selector)?;
    programmer.program(bitstream)?;
    programmer.close()?;

    let mut board = Board::open_selected(&selector)?;
    let fifo_words = usize::from(board.config().fifo_size_words());
    println!(
        "device programmed={} fifo={} vericomm={} clock_continues={}",
        board.config().is_programmed(),
        fifo_words,
        board.config().vericomm_ability(),
        board.config().vericomm_clock_continues()
    );
    let mut io = board.configure_io(&IoConfig::new(Licence::CustomerId(profile.customer_id)))?;

    let mut tx = vec![0u16; fifo_words];
    let mut rx = vec![0u16; fifo_words];
    tx[..VeriCommFrame::WORDS].copy_from_slice(VeriCommFrame::from_bits(0x0008).words());
    // Sample 0 asserts reset; subsequent samples leave it low while the clock free-runs.

    io.transfer_into(&tx, &mut rx)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    println!("sample | raw               | cnt(nibble)");
    let (samples, remainder) = rx.as_chunks::<{ VeriCommFrame::WORDS }>();
    debug_assert!(
        remainder.is_empty(),
        "VeriComm words must form whole frames"
    );
    for (index, words) in samples.iter().enumerate().take(n_samples) {
        let sample = VeriCommFrame::from_words(*words);
        let nibble = sample.words()[0] & 0xf;
        println!(
            "{index:6} | {:016x} | {nibble:x} {}",
            sample.bits(),
            "#".repeat(nibble as usize)
        );
    }

    io.finish()?;
    board.close()?;
    Ok(())
}
