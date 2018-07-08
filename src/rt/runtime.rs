use std::cell::Cell;

#[derive(Debug, Copy, Clone)]
pub enum RuntimeMode {
    ThreadPool,
    CurrentThread,
}

thread_local!(static MODE: Cell<Option<RuntimeMode>> = Cell::new(None));

struct ResetOnDrop(Option<RuntimeMode>);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        MODE.with(|mode| mode.set(self.0));
    }
}

pub(crate) fn with_set_mode<R>(mode: RuntimeMode, f: impl FnOnce() -> R) -> R {
    let prev = MODE.with(|m| m.replace(Some(mode)));
    let _reset = ResetOnDrop(prev);
    if prev.is_some() {
        panic!("The runtime mode has already set.");
    }
    f()
}

pub fn current_mode() -> Option<RuntimeMode> {
    MODE.with(|mode| mode.get())
}
