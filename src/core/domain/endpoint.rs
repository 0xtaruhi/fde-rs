#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointKind {
    Cell,
    Port,
    Unknown,
}

impl EndpointKind {
    pub fn classify(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "cell" => Self::Cell,
            "port" => Self::Port,
            _ => Self::Unknown,
        }
    }

    pub fn is_cell(self) -> bool {
        matches!(self, Self::Cell)
    }

    pub fn is_port(self) -> bool {
        matches!(self, Self::Port)
    }
}

#[cfg(test)]
mod tests {
    use super::EndpointKind;

    #[test]
    fn classifies_endpoint_kinds_case_insensitively() {
        assert_eq!(EndpointKind::classify("cell"), EndpointKind::Cell);
        assert_eq!(EndpointKind::classify("PORT"), EndpointKind::Port);
        assert_eq!(EndpointKind::classify("mystery"), EndpointKind::Unknown);
    }
}
