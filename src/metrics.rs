#[derive(Debug, Default)]
pub struct Metrics {
    pub count: usize,
    pub resized: usize,
    pub traversed: usize,
    pub skipped: usize,
}
