use crate::cil::{BitstreamCommand, Cil};
use anyhow::{Context, Result, anyhow};
use std::fmt::Write;

use super::encode::parameter_u32;
use super::model::MajorPayload;

pub(crate) fn render_bitstream_text(
    design_name: &str,
    device_name: &str,
    cil: &Cil,
    major_payloads: &[MajorPayload],
    memory_payloads: &[Vec<u32>],
    notes: &mut Vec<String>,
) -> Result<String> {
    let mut text = String::new();
    let mut emitted_majors = 0usize;
    let mut emitted_memories = 0usize;

    for command in &cil.bitstream_commands {
        match command.cmd.as_str() {
            "bsHeader" => write_header(&mut text, design_name, device_name)?,
            "adjustSYNC" => write_sync(&mut text, command.parameter.as_deref())?,
            "insertCMD" => write_insert_cmd(&mut text, command)?,
            "setFRMLen" => write_frame_length(&mut text, cil)?,
            "setOption" => write_single_word_command(
                &mut text,
                "3001_2001",
                required_command_parameter(command, "setOption")?,
                "configure option",
            )?,
            "shapeMask" => write_single_word_command(
                &mut text,
                "3000_C001",
                required_command_parameter(command, "shapeMask")?,
                "mask",
            )?,
            "dummy" => write_dummy(&mut text)?,
            "appointCTL" => write_single_word_command(
                &mut text,
                "3000_A001",
                required_command_parameter(command, "appointCTL")?,
                "CTL value",
            )?,
            "writeNomalTiles" => {
                emitted_majors += write_major_blocks(&mut text, cil, major_payloads)?;
            }
            "writeMem" => {
                emitted_memories += write_memory_blocks(&mut text, cil, memory_payloads)?;
            }
            other => notes.push(format!(
                "Unsupported bitstream command {other}; it is ignored."
            )),
        }
    }

    if emitted_majors != major_payloads.len() {
        notes.push(format!(
            "Bitstream command stream emitted {} major chunks for {} available majors.",
            emitted_majors,
            major_payloads.len()
        ));
    }
    if emitted_memories != memory_payloads.len() {
        notes.push(format!(
            "Bitstream command stream emitted {} memory chunks for {} available memory chunks.",
            emitted_memories,
            memory_payloads.len()
        ));
    }

    Ok(text)
}

pub(crate) fn hex_word(word: u32) -> String {
    format!("{:04x}_{:04x}", (word >> 16) & 0xffff, word & 0xffff)
}

fn required_command_parameter<'a>(command: &'a BitstreamCommand, name: &str) -> Result<&'a str> {
    command.parameter.as_deref().ok_or_else(|| {
        anyhow!(
            "bitstream command {} is missing its required parameter payload",
            name
        )
    })
}

fn write_header(text: &mut String, design_name: &str, device_name: &str) -> Result<()> {
    writeln!(text, "0000_0000\t// chip_type: {device_name}")?;
    writeln!(text, "0000_0000\t// bit file name: {design_name}.bit")?;
    writeln!(text, "0000_0000\t// current time: deterministic")?;
    writeln!(text, "0000_0000")?;
    writeln!(text, "0000_0000")?;
    writeln!(text, "0000_0000")?;
    Ok(())
}

fn write_sync(text: &mut String, parameter: Option<&str>) -> Result<()> {
    let sync_word = match parameter {
        Some(word) if !word.trim().is_empty() => word.trim(),
        _ => "AA99_5566",
    };
    writeln!(text, "{sync_word}")?;
    writeln!(text, "{sync_word}")?;
    Ok(())
}

fn write_insert_cmd(text: &mut String, command: &BitstreamCommand) -> Result<()> {
    let (word, comment) = split_command_parameter(command.parameter.as_deref());
    writeln!(text, "3000_8001")?;
    writeln!(text, "{word}\t//{comment}")?;
    Ok(())
}

fn write_frame_length(text: &mut String, cil: &Cil) -> Result<()> {
    writeln!(text, "3001_6001\t//frame length")?;
    writeln!(text, "{}", hex_word(parameter_u32(cil, "FRMLen")?))?;
    Ok(())
}

fn write_single_word_command(
    text: &mut String,
    opcode: &str,
    payload: &str,
    comment: &str,
) -> Result<()> {
    writeln!(text, "{opcode}")?;
    writeln!(text, "{payload}\t//{comment}")?;
    Ok(())
}

fn write_dummy(text: &mut String) -> Result<()> {
    writeln!(text, "3000_2001")?;
    writeln!(text, "0000_0000")?;
    Ok(())
}

fn write_major_blocks(
    text: &mut String,
    cil: &Cil,
    major_payloads: &[MajorPayload],
) -> Result<usize> {
    let major_shift = super::encode::parameter_usize(cil, "major_shift")
        .context("missing or invalid major_shift while rendering major blocks")?;
    for payload in major_payloads {
        let shifted_address = u32::try_from(payload.address << major_shift).map_err(|_| {
            anyhow!(
                "major address {} overflows 32-bit bitstream field",
                payload.address
            )
        })?;
        write_fdri_block(text, shifted_address, &payload.words)?;
    }
    Ok(major_payloads.len())
}

fn write_memory_blocks(
    text: &mut String,
    cil: &Cil,
    memory_payloads: &[Vec<u32>],
) -> Result<usize> {
    let word_count_shift = parameter_u32(cil, "wrdsAmnt_shift")
        .context("missing or invalid wrdsAmnt_shift while rendering memory blocks")?;
    let fillblank = super::encode::parameter_usize(cil, "fillblank")
        .context("missing or invalid fillblank while rendering memory blocks")?;

    for (index, payload) in memory_payloads.iter().enumerate() {
        let memory_address = 0x0200_0000u32 + (index as u32 + 1) * 0x0002_0000u32;
        write_fdri_block_with_padding(text, memory_address, payload, word_count_shift, fillblank)?;
    }
    Ok(memory_payloads.len())
}

fn write_fdri_block(text: &mut String, address: u32, payload: &[u32]) -> Result<()> {
    write_fdri_block_with_padding(text, address, payload, 0, 0)
}

fn write_fdri_block_with_padding(
    text: &mut String,
    address: u32,
    payload: &[u32],
    extra_words: u32,
    fillblank: usize,
) -> Result<()> {
    let word_length = u32::try_from(payload.len()).map_err(|_| {
        anyhow!(
            "payload of {} words does not fit in bitstream header",
            payload.len()
        )
    })?;
    let header_words = word_length + extra_words;

    writeln!(text, "3000_2001")?;
    writeln!(text, "{}", hex_word(address))?;
    writeln!(text, "3000_4000\t//{} words", header_words)?;
    writeln!(text, "{}", hex_word(0x5000_0000u32 + header_words))?;
    for word in payload {
        writeln!(text, "{}", hex_word(*word))?;
    }
    for _ in 0..fillblank {
        writeln!(text, "0000_0000")?;
    }
    Ok(())
}

fn split_command_parameter(parameter: Option<&str>) -> (&str, &str) {
    let Some(parameter) = parameter else {
        return ("", "");
    };
    match parameter.split_once(',') {
        Some((word, comment)) => (word.trim_start(), comment),
        None => (parameter.trim_start(), ""),
    }
}

#[cfg(test)]
mod tests {
    use super::{hex_word, split_command_parameter};

    #[test]
    fn renders_hex_words_in_frame_format() {
        assert_eq!(hex_word(0xaaff_ffff), "aaff_ffff");
    }

    #[test]
    fn splits_insert_command_comment_consistently() {
        let (word, comment) = split_command_parameter(Some("0000_0007, reset CRC"));
        assert_eq!(word, "0000_0007");
        assert_eq!(comment, " reset CRC");

        let (word, comment) = split_command_parameter(Some("0000_0005"));
        assert_eq!(word, "0000_0005");
        assert_eq!(comment, "");
    }
}
