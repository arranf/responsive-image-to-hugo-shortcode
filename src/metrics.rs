#[derive(Debug, Default)]
pub struct Metrics {
    pub count: u32,
    pub resized: u32,
    pub traversed: u32,
    pub skipped: u32,
}
