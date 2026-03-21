use anyhow::{Context, Result, anyhow, bail};
use std::{collections::BTreeMap, env, path::Path, thread, time::Duration};
use vlfd_rs::{Device, IoSettings, Programmer};

#[derive(Debug, Clone)]
struct Segment {
    label: String,
    pattern: u16,
    words: usize,
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
    if words == 0 {
        bail!("segment repeat count must be positive: {raw}");
    }
    Ok(Segment {
        label: raw.to_string(),
        pattern,
        words,
    })
}

fn decoded_mask(rx: &[u16]) -> u16 {
    let high_threshold = rx.len().saturating_mul(7) / 8;
    (0..16).fold(0u16, |mask, bit| {
        let count = rx
            .iter()
            .filter(|&&word| (word & (1u16 << bit)) != 0)
            .count();
        if count > high_threshold {
            mask | (1u16 << bit)
        } else {
            mask
        }
    })
}

fn summarize_segment(index: usize, segment: &Segment, rx: &[u16]) {
    let decoded = decoded_mask(rx);
    let low_nibble_hist = rx.iter().fold(BTreeMap::<u16, usize>::new(), |mut hist, word| {
        *hist.entry(word & 0x000f).or_default() += 1;
        hist
    });
    let preview = rx
        .iter()
        .take(8)
        .map(|word| format!("{word:04x}"))
        .collect::<Vec<_>>()
        .join(" ");
    let dominant_low = low_nibble_hist
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(value, count)| format!("0x{value:x} ({count}/{})", rx.len()))
        .unwrap_or_else(|| "n/a".to_string());

    println!(
        "segment[{index}] {} tx=0x{:04x} words={} decoded=0x{:04x} outputs=0x{:x}",
        segment.label,
        segment.pattern,
        segment.words,
        decoded,
        decoded & 0x000f,
    );
    println!("  dominant_low_nibble={dominant_low} preview={preview}");
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
    let mut args = env::args().skip(1);
    let bitstream = args
        .next()
        .context("usage: wave_probe <bitstream> [pattern*words ...]")?;
    let mut segments = args.map(|arg| parse_segment(&arg)).collect::<Result<Vec<_>>>()?;
    if segments.is_empty() {
        segments = default_segments();
    }

    let mut programmer = Programmer::connect()?;
    programmer.program(Path::new(&bitstream))?;
    programmer.close()?;
    println!("program_ok {bitstream}");

    let mut device = Device::connect()?;
    let fifo_words = usize::from(device.config().fifo_size());
    println!(
        "device_connected programmed={} fifo_size={} vericomm={} version={}",
        device.config().is_programmed(),
        fifo_words,
        device.config().vericomm_ability(),
        device.config().smims_version_raw(),
    );
    device.enter_io_mode(&IoSettings::default())?;
    thread::sleep(Duration::from_millis(50));

    let prime_words = fifo_words.min(64);
    let mut prime_tx = vec![0u16; prime_words];
    let mut prime_rx = vec![0u16; prime_words];
    device.transfer_io(&mut prime_tx, &mut prime_rx)?;
    thread::sleep(Duration::from_millis(20));

    let total_words = segments.iter().map(|segment| segment.words).sum::<usize>();
    if total_words > fifo_words {
        bail!(
            "waveform uses {total_words} words but device FIFO only holds {fifo_words}; reduce segment sizes"
        );
    }

    let mut tx = Vec::with_capacity(fifo_words);
    let mut ranges = Vec::with_capacity(segments.len());
    for segment in &segments {
        let start = tx.len();
        tx.extend(std::iter::repeat_n(segment.pattern, segment.words));
        let end = tx.len();
        ranges.push(start..end);
    }
    if tx.len() < fifo_words {
        let start = tx.len();
        let padding = fifo_words - start;
        tx.extend(std::iter::repeat_n(0u16, padding));
        ranges.push(start..fifo_words);
        segments.push(Segment {
            label: "tail_idle".to_string(),
            pattern: 0,
            words: padding,
        });
    }

    let mut rx = vec![0u16; tx.len()];
    device.transfer_io(&mut tx, &mut rx)?;
    thread::sleep(Duration::from_millis(20));

    for (index, (segment, range)) in segments.iter().zip(ranges.iter()).enumerate() {
        summarize_segment(index, segment, &rx[range.clone()]);
    }

    device.exit_io_mode()?;
    Ok(())
}
