//! Components for accessing HTTP requests and global/request-local data.

pub mod body;
pub mod local_map;

use std::{cell::Cell, ptr::NonNull};
pub use {
    self::body::RequestBody,
    crate::app::imp::{Cookies, Input, Params, State},
};

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

/// Returns `true` if the reference to `Input` is set to the current task.
#[inline(always)]
pub fn is_set_current() -> bool {
    INPUT.with(|input| input.get().is_some())
}

#[cfg_attr(feature = "cargo-clippy", allow(cast_ptr_alignment))]
pub(crate) fn with_set_current<R>(self_: &mut Input<'_>, f: impl FnOnce() -> R) -> R {
    // safety: The value of `self: &mut Input` is always non-null.
    let prev = INPUT.with(|input| {
        let ptr = self_ as *mut Input<'_> as *mut () as *mut Input<'static>;
        input.replace(Some(unsafe { NonNull::new_unchecked(ptr) }))
    });
    let _reset = ResetOnDrop(prev);
    f()
}

/// Acquires a mutable borrow of `Input` from the current task context and executes the provided
/// closure with its reference.
///
/// # Panics
///
/// This function only work in the management of the framework and causes a panic
/// if any references to `Input` is not set at the current task.
/// Do not use this function outside of futures returned by the handler functions.
/// Such situations often occurs by spawning tasks by the external `Executor`
/// (typically calling `tokio::spawn()`).
///
/// In additional, this function forms a (dynamic) scope to prevent the references to `Input`
/// violate the borrowing rule in Rust.
/// Duplicate borrowings such as the following code are reported as a runtime error.
///
/// ```ignore
/// with_get_current(|input| {
///     some_process()
/// });
///
/// fn some_process() {
///     // Duplicate borrowing of `Input` occurs at this point.
///     with_get_current(|input| { ... })
/// }
/// ```
pub fn with_get_current<R>(f: impl FnOnce(&mut Input<'_>) -> R) -> R {
    let input_ptr = INPUT.with(|input| input.replace(None));
    let _reset = ResetOnDrop(input_ptr);
    let mut input_ptr =
        input_ptr.expect("Any reference to Input are not set at the current task context.");
    // safety: The lifetime of `input_ptr` is always shorter then the borrowing of `Input` in `with_set_current()`
    f(unsafe { input_ptr.as_mut() })
}
