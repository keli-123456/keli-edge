use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Protocol {
    Naive,
    Mieru,
    Xray,
    Custom(String),
}

impl Protocol {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Naive => "naive",
            Self::Mieru => "mieru",
            Self::Xray => "xray",
            Self::Custom(value) => value.as_str(),
        }
    }

    pub fn needs_sidecar(&self) -> bool {
        matches!(self, Self::Naive | Self::Mieru | Self::Custom(_))
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Protocol {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err("protocol is empty".to_string());
        }

        Ok(match normalized.as_str() {
            "naive" => Self::Naive,
            "mieru" => Self::Mieru,
            "xray" => Self::Xray,
            _ => Self::Custom(normalized),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Protocol;
    use std::str::FromStr;

    #[test]
    fn parses_known_protocols() {
        assert_eq!(Protocol::from_str("naive").unwrap(), Protocol::Naive);
        assert_eq!(Protocol::from_str("MIERU").unwrap(), Protocol::Mieru);
    }

    #[test]
    fn custom_protocols_need_sidecars() {
        let protocol = Protocol::from_str("custom-core").unwrap();
        assert!(protocol.needs_sidecar());
        assert_eq!(protocol.as_str(), "custom-core");
    }
}
