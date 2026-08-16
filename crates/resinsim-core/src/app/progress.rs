pub trait SimProgress {
    fn stage_message(&self, msg: &str);
    fn layer_tick(&self, current: u32, total: u32);
    fn message(&self, msg: &str);
}

pub struct NullProgress;

impl SimProgress for NullProgress {
    fn stage_message(&self, _msg: &str) {}
    fn layer_tick(&self, _current: u32, _total: u32) {}
    fn message(&self, _msg: &str) {}
}
