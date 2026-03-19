use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservedPorts {
    pub api: u16,
    pub backend: u16,
    pub test_helper: u16,
    pub integration_harness: u16,
    pub lightwalletd_tunnel: u16,
}

impl ReservedPorts {
    #[must_use]
    pub fn new() -> Self {
        Self {
            api: 3180,
            backend: 3181,
            test_helper: 3182,
            integration_harness: 3183,
            lightwalletd_tunnel: 3184,
        }
    }
}

impl Default for ReservedPorts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ReservedPorts;

    #[test]
    fn exposes_reserved_zimppy_port_range() {
        assert_eq!(
            ReservedPorts::new(),
            ReservedPorts {
                api: 3180,
                backend: 3181,
                test_helper: 3182,
                integration_harness: 3183,
                lightwalletd_tunnel: 3184,
            }
        );
    }
}
