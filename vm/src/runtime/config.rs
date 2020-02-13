pub struct Config {
    pub perm_size: usize,
    pub young_size: usize,
    pub old_size: usize,
    pub blocking: Option<usize>,
    pub primary: Option<usize>,
    pub gc_workers: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            perm_size: 2 * 1024 * 1024,
            young_size: 4 * 1024 * 1024,
            old_size: 2 * 1024 * 1024,
            blocking: None,
            primary: None,
            gc_workers: None,
        }
    }
}
