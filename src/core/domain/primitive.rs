#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstantKind {
    Zero,
    One,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveKind {
    Lut { inputs: Option<usize> },
    FlipFlop,
    Latch,
    Constant(ConstantKind),
    Buffer,
    Io,
    GlobalClockBuffer,
    Generic,
    Unknown,
}

impl PrimitiveKind {
    pub fn classify(kind: &str, type_name: &str) -> Self {
        let kind_lower = kind.trim().to_ascii_lowercase();
        let type_upper = type_name.trim().to_ascii_uppercase();
        let type_lower = type_upper.to_ascii_lowercase();

        if kind_lower == "lut" || type_upper.starts_with("LUT") {
            return Self::Lut {
                inputs: parse_lut_inputs(type_name),
            };
        }
        if kind_lower.contains("latch") || type_lower.contains("latch") {
            return Self::Latch;
        }
        if kind_lower.contains("ff") || type_lower.contains("dff") || type_lower.contains("edff") {
            return Self::FlipFlop;
        }
        match type_upper.as_str() {
            "GND" => return Self::Constant(ConstantKind::Zero),
            "VCC" => return Self::Constant(ConstantKind::One),
            _ => {}
        }
        if kind_lower == "constant" {
            return Self::Constant(ConstantKind::Unknown);
        }
        if kind_lower == "gclk" || type_upper.contains("GCLK") {
            return Self::GlobalClockBuffer;
        }
        if kind_lower == "iob" || type_upper == "IOB" {
            return Self::Io;
        }
        if kind_lower == "buffer" || type_lower == "buffer" || type_lower == "buf" {
            return Self::Buffer;
        }
        if kind_lower == "generic" {
            return Self::Generic;
        }
        Self::Unknown
    }

    pub fn is_sequential(self) -> bool {
        matches!(self, Self::FlipFlop | Self::Latch)
    }

    pub fn is_lut(self) -> bool {
        matches!(self, Self::Lut { .. })
    }

    pub fn is_constant_source(self) -> bool {
        matches!(self, Self::Constant(_))
    }

    pub fn is_buffer(self) -> bool {
        matches!(self, Self::Buffer)
    }

    pub fn constant_kind(self) -> Option<ConstantKind> {
        match self {
            Self::Constant(kind) => Some(kind),
            _ => None,
        }
    }

    pub fn lut_input_index(self, pin: &str) -> Option<usize> {
        if !self.is_lut() {
            return None;
        }
        let pin = pin.trim().to_ascii_uppercase();
        if let Some(value) = pin.strip_prefix('I') {
            return value.parse().ok();
        }
        match pin.as_str() {
            "ADR0" => Some(0),
            "ADR1" => Some(1),
            "ADR2" => Some(2),
            "ADR3" => Some(3),
            "A" | "A1" => Some(0),
            "B" | "A2" => Some(1),
            "C" | "A3" => Some(2),
            "D" | "A4" => Some(3),
            _ => None,
        }
    }

    pub fn is_lut_output_pin(self, pin: &str) -> bool {
        self.is_lut()
            && matches!(
                pin.trim().to_ascii_uppercase().as_str(),
                "O" | "Y" | "OUT" | "Q"
            )
    }

    pub fn is_register_output_pin(self, pin: &str) -> bool {
        self.is_sequential() && pin.trim().eq_ignore_ascii_case("Q")
    }

    pub fn is_clock_pin(self, pin: &str) -> bool {
        self.is_sequential() && matches!(pin.trim().to_ascii_uppercase().as_str(), "CK" | "CLK")
    }

    pub fn is_clock_enable_pin(self, pin: &str) -> bool {
        self.is_sequential() && pin.trim().eq_ignore_ascii_case("CE")
    }

    pub fn is_set_reset_pin(self, pin: &str) -> bool {
        self.is_sequential()
            && matches!(
                pin.trim().to_ascii_uppercase().as_str(),
                "R" | "S" | "SR" | "RST" | "RESET"
            )
    }

    pub fn is_register_data_pin(self, pin: &str) -> bool {
        self.is_sequential() && pin.trim().eq_ignore_ascii_case("D")
    }
}

fn parse_lut_inputs(type_name: &str) -> Option<usize> {
    let digits = type_name
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{ConstantKind, PrimitiveKind};

    #[test]
    fn classifies_primitives_and_common_pins() {
        let lut = PrimitiveKind::classify("lut", "LUT4");
        assert!(lut.is_lut());
        assert_eq!(lut.lut_input_index("ADR2"), Some(2));
        assert!(lut.is_lut_output_pin("out"));

        let ff = PrimitiveKind::classify("logic_ff", "DFF");
        assert!(ff.is_sequential());
        assert!(ff.is_register_output_pin("Q"));
        assert!(ff.is_clock_pin("clk"));
        assert!(ff.is_register_data_pin("D"));

        let gnd = PrimitiveKind::classify("constant", "GND");
        assert_eq!(gnd.constant_kind(), Some(ConstantKind::Zero));

        let generic = PrimitiveKind::classify("generic", "mystery");
        assert_eq!(generic, PrimitiveKind::Generic);
    }
}
