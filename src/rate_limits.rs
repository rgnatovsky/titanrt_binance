use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct Window {
    limit: u32,
    window_ms: u64,
    window_index: u64,
    used: u32,
}

impl Window {
    pub fn update_from_response(&mut self, server_ms: u64, count: u32) {
        let idx = server_ms / self.window_ms;
        if idx != self.window_index {
            self.window_index = idx;
            self.used = 0;
        }
        self.used = count; // доверяем серверу
    }

    pub fn deficit(&self, cost: u32) -> Option<u64> {
        if self.used + cost <= self.limit {
            return None;
        }
        let now_ms = /* возьми serverTime из ответа */ 0;
        Some(((self.window_index + 1) * self.window_ms).saturating_sub(now_ms))
    }
}
