use universe_sdk::{Universe, UniverseManifest};

universe_sdk::universe_exports!(EchoSession);

struct EchoSession {
    tick_count: u32,
}

impl Universe for EchoSession {
    fn create(_params: &str) -> Self {
        EchoSession { tick_count: 0 }
    }

    fn tick(&mut self, input: &str) -> String {
        self.tick_count += 1;
        // Avoid serde_json for the hot path to keep binary size minimal.
        let tick = self.tick_count;
        let safe = input.replace('"', "\\\"");
        format!("{{\"echo\":\"{safe}\",\"tick\":{tick}}}")
    }

    fn manifest() -> UniverseManifest {
        UniverseManifest {
            name: "echo".into(),
            version: "0.1.0".into(),
            api_version: 1,
            max_players: 1,
            capabilities: vec![],
        }
    }
}
