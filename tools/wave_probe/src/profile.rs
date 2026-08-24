use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vlfd_rs::VeriCommFrame;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PinLane {
    pub pin: String,
    pub lane: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BoardProfile {
    pub name: String,
    pub customer_id: u16,
    pub clock_pin: String,
    pub frame_words: usize,
    pub clock_continues: bool,
    pub inputs: Vec<PinLane>,
    pub outputs: Vec<PinLane>,
}

impl BoardProfile {
    pub fn bundled() -> Result<Self> {
        let profile: Self = serde_json::from_str(include_str!("../board_profiles/fdp3p7.json"))?;
        profile.validate()?;
        Ok(profile)
    }

    fn validate(&self) -> Result<()> {
        if self.name.is_empty() || self.clock_pin.is_empty() || self.customer_id == 0 {
            bail!("board profile name, clock pin, and customer ID must be set");
        }
        if self.frame_words != VeriCommFrame::WORDS {
            bail!(
                "board profile uses {} words per frame, expected {}",
                self.frame_words,
                VeriCommFrame::WORDS
            );
        }
        validate_pin_lanes("input", &self.inputs)?;
        validate_pin_lanes("output", &self.outputs)
    }
}

fn validate_pin_lanes(kind: &str, entries: &[PinLane]) -> Result<()> {
    let mut pins = BTreeSet::new();
    let mut lanes = BTreeSet::new();
    for entry in entries {
        if entry.pin.is_empty() || entry.lane >= VeriCommFrame::LANES {
            bail!("invalid {kind} mapping {} -> {}", entry.pin, entry.lane);
        }
        if !pins.insert(entry.pin.as_str()) || !lanes.insert(entry.lane) {
            bail!("duplicate {kind} pin or lane in board profile");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::BoardProfile;

    #[test]
    fn bundled_profile_covers_the_fixture_io_surface() {
        let profile = BoardProfile::bundled().expect("profile should be valid");
        assert_eq!(profile.customer_id, 0xf805);
        assert_eq!(profile.clock_pin, "P77");
        assert_eq!(profile.inputs.len(), 54);
        assert_eq!(profile.outputs.len(), 54);
        assert_eq!(profile.inputs[0].pin, "P151");
        assert_eq!(profile.outputs[53].pin, "P194");
    }
}
