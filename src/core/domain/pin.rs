use super::{PrimitiveKind, SiteKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinRole {
    LutInput(usize),
    LutOutput,
    RegisterOutput,
    RegisterClock,
    RegisterClockEnable,
    RegisterSetReset,
    RegisterData,
    SiteInput,
    SiteOutput,
    GlobalClockInput,
    GlobalClockOutput,
    GeneralOutput,
    Unknown,
}

impl PinRole {
    pub fn classify_for_primitive(primitive: PrimitiveKind, pin: &str) -> Self {
        if let Some(index) = primitive.lut_input_index(pin) {
            return Self::LutInput(index);
        }
        if primitive.is_lut_output_pin(pin) {
            return Self::LutOutput;
        }
        if primitive.is_register_output_pin(pin) {
            return Self::RegisterOutput;
        }
        if primitive.is_clock_pin(pin) {
            return Self::RegisterClock;
        }
        if primitive.is_clock_enable_pin(pin) {
            return Self::RegisterClockEnable;
        }
        if primitive.is_set_reset_pin(pin) {
            return Self::RegisterSetReset;
        }
        if primitive.is_register_data_pin(pin) {
            return Self::RegisterData;
        }
        Self::Unknown
    }

    pub fn classify_for_site(site_kind: SiteKind, pin: &str) -> Self {
        let pin = pin.trim().to_ascii_uppercase();
        match site_kind {
            SiteKind::Iob => match pin.as_str() {
                "IN" => Self::SiteInput,
                "OUT" => Self::SiteOutput,
                _ => Self::Unknown,
            },
            SiteKind::GclkIob => match pin.as_str() {
                "GCLKOUT" => Self::GlobalClockOutput,
                _ => Self::Unknown,
            },
            SiteKind::Gclk => match pin.as_str() {
                "IN" => Self::GlobalClockInput,
                "OUT" => Self::GlobalClockOutput,
                _ => Self::Unknown,
            },
            SiteKind::LogicSlice | SiteKind::Unknown => Self::Unknown,
        }
    }

    pub fn classify_output_pin(primitive: PrimitiveKind, pin: &str) -> Self {
        let role = Self::classify_for_primitive(primitive, pin);
        if role.is_output_like() {
            return role;
        }
        if primitive.is_constant_source() {
            return Self::GeneralOutput;
        }
        match pin.trim().to_ascii_uppercase().as_str() {
            "Q" | "O" | "Y" | "OUT" | "P" | "G" => Self::GeneralOutput,
            _ => Self::Unknown,
        }
    }

    pub fn lut_input_index(self) -> Option<usize> {
        match self {
            Self::LutInput(index) => Some(index),
            _ => None,
        }
    }

    pub fn is_output_like(self) -> bool {
        matches!(
            self,
            Self::LutOutput | Self::RegisterOutput | Self::GlobalClockOutput | Self::GeneralOutput
        )
    }

    pub fn is_site_input(self) -> bool {
        matches!(self, Self::SiteInput)
    }

    pub fn is_site_output(self) -> bool {
        matches!(self, Self::SiteOutput)
    }

    pub fn is_global_clock_input(self) -> bool {
        matches!(self, Self::GlobalClockInput)
    }

    pub fn is_global_clock_output(self) -> bool {
        matches!(self, Self::GlobalClockOutput)
    }
}

#[cfg(test)]
mod tests {
    use super::PinRole;
    use crate::domain::{PrimitiveKind, SiteKind};

    #[test]
    fn classifies_primitive_and_site_pins() {
        let lut = PrimitiveKind::classify("lut", "LUT4");
        assert_eq!(
            PinRole::classify_for_primitive(lut, "ADR1"),
            PinRole::LutInput(1)
        );
        assert_eq!(
            PinRole::classify_for_primitive(lut, "O"),
            PinRole::LutOutput
        );

        let ff = PrimitiveKind::classify("ff", "DFF");
        assert_eq!(
            PinRole::classify_for_primitive(ff, "CLK"),
            PinRole::RegisterClock
        );
        assert_eq!(
            PinRole::classify_for_primitive(ff, "Q"),
            PinRole::RegisterOutput
        );

        assert_eq!(
            PinRole::classify_for_site(SiteKind::Iob, "IN"),
            PinRole::SiteInput
        );
        assert_eq!(
            PinRole::classify_for_site(SiteKind::Gclk, "OUT"),
            PinRole::GlobalClockOutput
        );
    }
}
