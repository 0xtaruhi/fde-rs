use super::{
    CellKind,
    ascii::{
        trimmed_contains_ignore_ascii_case, trimmed_eq_ignore_ascii_case,
        trimmed_starts_with_ignore_ascii_case, trimmed_strip_prefix_ignore_ascii_case,
    },
};

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
        Self::from_cell_kind(CellKind::classify(kind), type_name)
    }

    pub fn from_cell_kind(kind: CellKind, type_name: &str) -> Self {
        let type_name = type_name.trim();

        if matches!(kind, CellKind::Lut) || trimmed_starts_with_ignore_ascii_case(type_name, "LUT")
        {
            return Self::Lut {
                inputs: parse_lut_inputs(type_name),
            };
        }
        if matches!(kind, CellKind::Latch) || trimmed_contains_ignore_ascii_case(type_name, "latch")
        {
            return Self::Latch;
        }
        if matches!(kind, CellKind::Ff)
            || trimmed_contains_ignore_ascii_case(type_name, "dff")
            || trimmed_contains_ignore_ascii_case(type_name, "edff")
        {
            return Self::FlipFlop;
        }
        if trimmed_eq_ignore_ascii_case(type_name, "GND") {
            return Self::Constant(ConstantKind::Zero);
        }
        if trimmed_eq_ignore_ascii_case(type_name, "VCC") {
            return Self::Constant(ConstantKind::One);
        }
        if matches!(kind, CellKind::Constant) {
            return Self::Constant(ConstantKind::Unknown);
        }
        if matches!(kind, CellKind::GlobalClockBuffer)
            || trimmed_contains_ignore_ascii_case(type_name, "GCLK")
        {
            return Self::GlobalClockBuffer;
        }
        if matches!(kind, CellKind::Io) || trimmed_eq_ignore_ascii_case(type_name, "IOB") {
            return Self::Io;
        }
        if matches!(kind, CellKind::Buffer)
            || trimmed_eq_ignore_ascii_case(type_name, "buffer")
            || trimmed_eq_ignore_ascii_case(type_name, "buf")
        {
            return Self::Buffer;
        }
        if matches!(kind, CellKind::Generic) {
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
        if let Some(value) = trimmed_strip_prefix_ignore_ascii_case(pin, "I") {
            return value.parse().ok();
        }
        if trimmed_eq_ignore_ascii_case(pin, "ADR0") {
            Some(0)
        } else if trimmed_eq_ignore_ascii_case(pin, "ADR1") {
            Some(1)
        } else if trimmed_eq_ignore_ascii_case(pin, "ADR2") {
            Some(2)
        } else if trimmed_eq_ignore_ascii_case(pin, "ADR3") {
            Some(3)
        } else if trimmed_eq_ignore_ascii_case(pin, "A") || trimmed_eq_ignore_ascii_case(pin, "A1")
        {
            Some(0)
        } else if trimmed_eq_ignore_ascii_case(pin, "B") || trimmed_eq_ignore_ascii_case(pin, "A2")
        {
            Some(1)
        } else if trimmed_eq_ignore_ascii_case(pin, "C") || trimmed_eq_ignore_ascii_case(pin, "A3")
        {
            Some(2)
        } else if trimmed_eq_ignore_ascii_case(pin, "D") || trimmed_eq_ignore_ascii_case(pin, "A4")
        {
            Some(3)
        } else {
            None
        }
    }

    pub fn is_lut_output_pin(self, pin: &str) -> bool {
        self.is_lut()
            && (trimmed_eq_ignore_ascii_case(pin, "O")
                || trimmed_eq_ignore_ascii_case(pin, "Y")
                || trimmed_eq_ignore_ascii_case(pin, "OUT")
                || trimmed_eq_ignore_ascii_case(pin, "Q"))
    }

    pub fn is_register_output_pin(self, pin: &str) -> bool {
        self.is_sequential() && trimmed_eq_ignore_ascii_case(pin, "Q")
    }

    pub fn is_clock_pin(self, pin: &str) -> bool {
        self.is_sequential()
            && (trimmed_eq_ignore_ascii_case(pin, "CK") || trimmed_eq_ignore_ascii_case(pin, "CLK"))
    }

    pub fn is_clock_enable_pin(self, pin: &str) -> bool {
        self.is_sequential() && trimmed_eq_ignore_ascii_case(pin, "CE")
    }

    pub fn is_set_reset_pin(self, pin: &str) -> bool {
        self.is_sequential()
            && (trimmed_eq_ignore_ascii_case(pin, "R")
                || trimmed_eq_ignore_ascii_case(pin, "S")
                || trimmed_eq_ignore_ascii_case(pin, "SR")
                || trimmed_eq_ignore_ascii_case(pin, "RST")
                || trimmed_eq_ignore_ascii_case(pin, "RESET"))
    }

    pub fn is_register_data_pin(self, pin: &str) -> bool {
        self.is_sequential() && trimmed_eq_ignore_ascii_case(pin, "D")
    }
}

fn parse_lut_inputs(type_name: &str) -> Option<usize> {
    let digit_offset = type_name
        .trim()
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_digit())
        .map(|(index, _)| index)?;
    type_name.trim().get(digit_offset..)?.parse().ok()
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
