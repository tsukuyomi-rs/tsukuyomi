//! **\[experimental\]**
//! Modifiers for supporting session management.

#![allow(missing_docs)]

pub mod cookie;
pub mod redis;

use crate::local_key;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Session {
    state: SessionState,
}

#[derive(Debug)]
enum SessionState {
    Empty,
    Some(HashMap<String, String>),
    Clear,
}

impl Session {
    local_key!(const KEY: Self);

    fn empty() -> Self {
        Self {
            state: SessionState::Empty,
        }
    }

    fn some(map: HashMap<String, String>) -> Self {
        Self {
            state: SessionState::Some(map),
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        match self.state {
            SessionState::Some(ref map) => map.get(name).map(|s| &**s),
            _ => None,
        }
    }

    pub fn set(&mut self, name: &str, value: String) -> Option<String> {
        match self.state {
            SessionState::Empty => {}
            SessionState::Some(ref mut map) => return map.insert(name.to_owned(), value),
            SessionState::Clear => return None,
        }

        match std::mem::replace(&mut self.state, SessionState::Empty) {
            SessionState::Empty => {
                self.state = SessionState::Some({
                    let mut map = HashMap::new();
                    map.insert(name.to_owned(), value);
                    map
                });
                None
            }
            SessionState::Some(..) | SessionState::Clear => unreachable!(),
        }
    }

    pub fn remove(&mut self, name: &str) {
        if let SessionState::Some(ref mut map) = self.state {
            map.remove(name);
        }
    }

    pub fn clear(&mut self) {
        self.state = SessionState::Clear;
    }
}

pub use self::ext::InputSessionExt;

mod ext {
    use super::Session;
    use crate::input::Input;

    #[cfg_attr(feature = "cargo-clippy", allow(stutter))]
    pub trait InputSessionExt: Sealed {
        fn session(&mut self) -> Option<&mut Session>;
    }

    impl<'a> InputSessionExt for Input<'a> {
        fn session(&mut self) -> Option<&mut Session> {
            self.locals_mut().get_mut(&Session::KEY)
        }
    }

    pub trait Sealed {}
    impl<'a> Sealed for Input<'a> {}
}
