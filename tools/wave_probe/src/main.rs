use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    env,
    fs::File,
    io::{BufWriter, Write},
    ops::Range,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use vlfd_rs::{Board, BoardInfo, BoardSelector, IoConfig, Licence, Programmer, VeriCommFrame};
use wave_probe::profile::{BoardProfile, PinLane};

const USAGE: &str = "usage: wave_probe [--trace-jsonl PATH] <bitstream> [pattern*words ...]";

#[derive(Debug, Clone)]
struct Segment {
    label: String,
    pattern: u16,
    words: usize,
}

#[derive(Debug)]
struct Options {
    bitstream: PathBuf,
    trace_jsonl: Option<PathBuf>,
    segments: Vec<Segment>,
}

fn parse_pattern(raw: &str) -> Result<u16> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        return u16::from_str_radix(hex, 16).with_context(|| format!("invalid hex pattern: {raw}"));
    }
    if let Some(bin) = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B")) {
        return u16::from_str_radix(bin, 2)
            .with_context(|| format!("invalid binary pattern: {raw}"));
    }
    raw.parse::<u16>()
        .with_context(|| format!("invalid decimal pattern: {raw}"))
}

fn parse_segment(raw: &str) -> Result<Segment> {
    let (pattern_text, words_text) = raw
        .split_once('*')
        .ok_or_else(|| anyhow!("segment must use PATTERN*WORDS syntax: {raw}"))?;
    let pattern = parse_pattern(pattern_text)?;
    let words = words_text
        .parse::<usize>()
        .with_context(|| format!("invalid repeat count in segment: {raw}"))?;
    if words == 0 || !words.is_multiple_of(VeriCommFrame::WORDS) {
        bail!(
            "segment word count must be a positive multiple of {}: {raw}",
            VeriCommFrame::WORDS
        );
    }
    Ok(Segment {
        label: raw.to_string(),
        pattern,
        words,
    })
}

fn parse_options(mut args: impl Iterator<Item = String>) -> Result<Options> {
    let mut bitstream = None;
    let mut trace_jsonl = None;
    let mut segments = Vec::new();

    while let Some(arg) = args.next() {
        if arg == "--trace-jsonl" {
            trace_jsonl = Some(PathBuf::from(
                args.next().context("--trace-jsonl requires a path")?,
            ));
        } else if arg.starts_with('-') {
            bail!("unknown option {arg}\n{USAGE}");
        } else if bitstream.is_none() {
            bitstream = Some(PathBuf::from(arg));
        } else {
            segments.push(parse_segment(&arg)?);
        }
    }

    if segments.is_empty() {
        segments = default_segments();
    }
    Ok(Options {
        bitstream: bitstream.context(USAGE)?,
        trace_jsonl,
        segments,
    })
}

fn frame(chunk: &[u16]) -> VeriCommFrame {
    VeriCommFrame::from_words([chunk[0], chunk[1], chunk[2], chunk[3]])
}

fn decoded_frame(rx: &[u16]) -> VeriCommFrame {
    let sample_count = rx.len() / VeriCommFrame::WORDS;
    let high_threshold = sample_count.saturating_mul(7) / 8;
    let mut decoded = VeriCommFrame::ZERO;
    for lane in 0..VeriCommFrame::LANES {
        let high = rx
            .chunks_exact(VeriCommFrame::WORDS)
            .filter(|sample| frame(sample).lane(lane) == Some(true))
            .count()
            > high_threshold;
        decoded.set_lane(lane, high);
    }
    decoded
}

fn summarize_segment(index: usize, segment: &Segment, rx: &[u16]) {
    let decoded = decoded_frame(rx);
    let samples = rx.len() / VeriCommFrame::WORDS;
    let low_nibble_hist = rx.chunks_exact(VeriCommFrame::WORDS).fold(
        BTreeMap::<u16, usize>::new(),
        |mut hist, words| {
            *hist.entry(words[0] & 0x000f).or_default() += 1;
            hist
        },
    );
    let preview = rx
        .chunks_exact(VeriCommFrame::WORDS)
        .take(8)
        .map(|words| format!("{:016x}", frame(words).bits()))
        .collect::<Vec<_>>()
        .join(" ");
    let dominant_low = low_nibble_hist
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(value, count)| format!("0x{value:x} ({count}/{samples})"))
        .unwrap_or_else(|| "n/a".to_string());

    println!(
        "segment[{index}] {} tx=0x{:04x} words={} samples={} decoded=0x{:016x} outputs=0x{:x}",
        segment.label,
        segment.pattern,
        segment.words,
        samples,
        decoded.bits(),
        decoded.words()[0] & 0x000f,
    );
    println!("  dominant_low_nibble={dominant_low} preview={preview}");
}

fn build_transfer(
    segments: &mut Vec<Segment>,
    fifo_words: usize,
) -> Result<(Vec<u16>, Vec<Range<usize>>)> {
    if !fifo_words.is_multiple_of(VeriCommFrame::WORDS) {
        bail!("device FIFO size {fifo_words} is not frame aligned");
    }
    let total_words = segments.iter().map(|segment| segment.words).sum::<usize>();
    if total_words > fifo_words {
        bail!(
            "waveform uses {total_words} words but device FIFO only holds {fifo_words}; reduce segment sizes"
        );
    }

    let mut tx = Vec::with_capacity(fifo_words);
    let mut ranges = Vec::with_capacity(segments.len() + 1);
    for segment in segments.iter() {
        let start = tx.len() / VeriCommFrame::WORDS;
        let sample = VeriCommFrame::from_bits(segment.pattern as u64);
        for _ in 0..segment.words / VeriCommFrame::WORDS {
            tx.extend_from_slice(sample.words());
        }
        ranges.push(start..tx.len() / VeriCommFrame::WORDS);
    }
    if tx.len() < fifo_words {
        let start = tx.len() / VeriCommFrame::WORDS;
        let padding = fifo_words - tx.len();
        tx.resize(fifo_words, 0);
        ranges.push(start..fifo_words / VeriCommFrame::WORDS);
        segments.push(Segment {
            label: "tail_idle".to_string(),
            pattern: 0,
            words: padding,
        });
    }
    Ok((tx, ranges))
}

#[derive(Serialize)]
struct TraceMetadata<'a> {
    kind: &'static str,
    schema: &'static str,
    profile: &'a str,
    sample_timing: &'static str,
    clock_continues: bool,
    transfer_started_ns: u128,
    transfer_completed_ns: u128,
    board: TraceBoard<'a>,
    clock_pin: &'a str,
    inputs: &'a [PinLane],
    outputs: &'a [PinLane],
}

#[derive(Serialize)]
struct TraceBoard<'a> {
    usb_bus_id: &'a str,
    usb_port_chain: &'a [u8],
    usb_address: u8,
    usb_serial_number: Option<&'a str>,
}

#[derive(Serialize)]
struct TraceSample {
    kind: &'static str,
    index: usize,
    segment: usize,
    tx: String,
    rx: String,
}

struct TraceBatch<'a> {
    profile: &'a BoardProfile,
    board: &'a BoardInfo,
    clock_continues: bool,
    transfer_started_ns: u128,
    transfer_completed_ns: u128,
    tx: &'a [u16],
    rx: &'a [u16],
    ranges: &'a [Range<usize>],
}

fn write_trace(path: &Path, batch: &TraceBatch<'_>) -> Result<()> {
    let mut writer = BufWriter::new(
        File::create(path).with_context(|| format!("could not create {}", path.display()))?,
    );
    serde_json::to_writer(
        &mut writer,
        &TraceMetadata {
            kind: "metadata",
            schema: "fde.vericomm-trace.v1",
            profile: &batch.profile.name,
            sample_timing: "asynchronous_usb_batch",
            clock_continues: batch.clock_continues,
            transfer_started_ns: batch.transfer_started_ns,
            transfer_completed_ns: batch.transfer_completed_ns,
            board: TraceBoard {
                usb_bus_id: &batch.board.location.bus_id,
                usb_port_chain: &batch.board.location.port_chain,
                usb_address: batch.board.address,
                usb_serial_number: batch.board.serial_number.as_deref(),
            },
            clock_pin: &batch.profile.clock_pin,
            inputs: &batch.profile.inputs,
            outputs: &batch.profile.outputs,
        },
    )?;
    writeln!(writer)?;

    for (index, (tx, rx)) in batch
        .tx
        .chunks_exact(VeriCommFrame::WORDS)
        .zip(batch.rx.chunks_exact(VeriCommFrame::WORDS))
        .enumerate()
    {
        let segment = batch
            .ranges
            .iter()
            .position(|range| range.contains(&index))
            .context("trace sample is outside every segment")?;
        serde_json::to_writer(
            &mut writer,
            &TraceSample {
                kind: "sample",
                index,
                segment,
                tx: format!("0x{:016x}", frame(tx).bits()),
                rx: format!("0x{:016x}", frame(rx).bits()),
            },
        )?;
        writeln!(writer)?;
    }
    writer.flush()?;
    Ok(())
}

fn default_segments() -> Vec<Segment> {
    [
        ("idle0", 0x0000, 64),
        ("din1", 0x0008, 64),
        ("clk1_din1", 0x000c, 64),
        ("clk0_din1", 0x0008, 64),
        ("din0", 0x0000, 64),
        ("clk1_din0", 0x0004, 64),
        ("clk0_din0", 0x0000, 64),
        ("din1_b", 0x0008, 64),
        ("clk1_din1_b", 0x000c, 64),
        ("clk0_din1_b", 0x0008, 64),
        ("din1_c", 0x0008, 64),
        ("clk1_din1_c", 0x000c, 64),
        ("clk0_din1_c", 0x0008, 64),
    ]
    .into_iter()
    .map(|(label, pattern, words)| Segment {
        label: label.to_string(),
        pattern,
        words,
    })
    .collect()
}

fn main() -> Result<()> {
    let mut options = parse_options(env::args().skip(1))?;
    let profile = BoardProfile::bundled()?;
    let boards = Board::enumerate()?;
    let [board_info] = boards.as_slice() else {
        bail!(
            "wave_probe requires exactly one connected board, found {}",
            boards.len()
        );
    };
    let selector = BoardSelector::UsbLocation(board_info.location.clone());

    let mut programmer = Programmer::open_selected(&selector)?;
    programmer.program(&options.bitstream)?;
    programmer.close()?;
    println!("program_ok {}", options.bitstream.display());

    let mut board = Board::open_selected(&selector)?;
    let fifo_words = usize::from(board.config().fifo_size_words());
    let clock_continues = board.config().vericomm_clock_continues();
    if clock_continues != profile.clock_continues {
        bail!(
            "board reports clock_continues={clock_continues}, profile expects {}",
            profile.clock_continues
        );
    }
    println!(
        "device_connected profile={} programmed={} fifo_size={} vericomm={} version={} clock_continues={}",
        profile.name,
        board.config().is_programmed(),
        fifo_words,
        board.config().vericomm_ability(),
        board.config().smims_version_raw(),
        clock_continues,
    );
    let mut io = board.configure_io(&IoConfig::new(Licence::CustomerId(profile.customer_id)))?;
    thread::sleep(Duration::from_millis(50));

    let prime_words = fifo_words.min(64) / VeriCommFrame::WORDS * VeriCommFrame::WORDS;
    if prime_words == 0 {
        bail!("device FIFO is too small for one VeriComm frame");
    }
    let prime_tx = vec![0u16; prime_words];
    let mut prime_rx = vec![0u16; prime_words];
    io.transfer_into(&prime_tx, &mut prime_rx)?;
    thread::sleep(Duration::from_millis(20));

    let (tx, ranges) = build_transfer(&mut options.segments, fifo_words)?;
    let mut rx = vec![0u16; tx.len()];
    let transfer_started = Instant::now();
    let transfer_started_ns = 0;
    io.transfer_into(&tx, &mut rx)?;
    let transfer_completed_ns = transfer_started.elapsed().as_nanos();
    thread::sleep(Duration::from_millis(20));

    for (index, (segment, range)) in options.segments.iter().zip(ranges.iter()).enumerate() {
        let words = range.start * VeriCommFrame::WORDS..range.end * VeriCommFrame::WORDS;
        summarize_segment(index, segment, &rx[words]);
    }

    if let Some(path) = options.trace_jsonl.as_deref() {
        write_trace(
            path,
            &TraceBatch {
                profile: &profile,
                board: board_info,
                clock_continues,
                transfer_started_ns,
                transfer_completed_ns,
                tx: &tx,
                rx: &rx,
                ranges: &ranges,
            },
        )?;
        println!("trace_jsonl {}", path.display());
    }

    io.finish()?;
    board.close()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Segment, build_transfer, decoded_frame, parse_segment};
    use vlfd_rs::VeriCommFrame;

    #[test]
    fn segments_require_complete_frames() {
        assert!(parse_segment("0x1*4").is_ok());
        assert!(parse_segment("0x1*3").is_err());
    }

    #[test]
    fn transfer_patterns_only_drive_the_first_sixteen_lanes() {
        let mut segments = vec![Segment {
            label: "test".into(),
            pattern: 0x000a,
            words: 8,
        }];
        let (tx, ranges) = build_transfer(&mut segments, 8).unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], 0..2);
        assert_eq!(tx, [0x000a, 0, 0, 0, 0x000a, 0, 0, 0]);
    }

    #[test]
    fn decoding_considers_every_lane_in_each_frame() {
        let sample = VeriCommFrame::from_bits(0x003f_ffff_ffff_fff8);
        let rx = sample
            .words()
            .iter()
            .copied()
            .cycle()
            .take(VeriCommFrame::WORDS * 8)
            .collect::<Vec<_>>();
        assert_eq!(decoded_frame(&rx), sample);
    }
}
