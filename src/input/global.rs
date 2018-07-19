use super::Input;
use std::cell::Cell;
use std::ptr::NonNull;

thread_local! {
    static INPUT: Cell<Option<NonNull<Input<'static>>>> = Cell::new(None);
}

#[allow(missing_debug_implementations)]
struct ResetOnDrop(Option<NonNull<Input<'static>>>);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        INPUT.with(|input| {
            input.set(self.0.take());
        })
    }
}

#[inline(always)]
pub(crate) fn is_set_current() -> bool {
    INPUT.with(|input| input.get().is_some())
}

#[cfg_attr(feature = "cargo-clippy", allow(cast_ptr_alignment))]
pub(super) fn with_set_current<R>(self_: &mut Input, f: impl FnOnce() -> R) -> R {
    // safety: The value of `self: &mut Input` is always non-null.
    let prev = INPUT.with(|input| {
        let ptr = self_ as *mut Input as *mut () as *mut Input<'static>;
        input.replace(Some(unsafe { NonNull::new_unchecked(ptr) }))
    });
    let _reset = ResetOnDrop(prev);
    f()
}

pub(super) fn with_get_current<R>(f: impl FnOnce(&mut Input) -> R) -> R {
    let input_ptr = INPUT.with(|input| input.replace(None));
    let _reset = ResetOnDrop(input_ptr);
    let mut input_ptr = input_ptr.expect("Any reference to Input are not set at the current task context.");
    // safety: The lifetime of `input_ptr` is always shorter then the borrowing of `Input` in `with_set_current()`
    f(unsafe { input_ptr.as_mut() })
}
