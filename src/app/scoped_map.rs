use std::any::TypeId;
use std::collections::hash_map::{Entry, HashMap};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};

use super::ScopeId;

// ==== ScopedValue ====

struct TypedScopedValue<T> {
    global: Option<T>,
    locals: Vec<Option<T>>,
    forward_ids: Vec<Option<ScopeId>>,
}

#[cfg_attr(tarpaulin, skip)]
impl<T> fmt::Debug for TypedScopedValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let locals = self.locals.iter().map(|_| "<value>").collect::<Vec<_>>();
        f.debug_struct("TypedScopedValue")
            .field("global", &self.global.as_ref().map(|_| "<value>"))
            .field("locals", &locals)
            .field("forward_ids", &self.forward_ids)
            .finish()
    }
}

impl<T> TypedScopedValue<T> {
    fn new(value: T, id: ScopeId) -> TypedScopedValue<T> {
        match id {
            ScopeId::Global => Self::new_global(value),
            ScopeId::Local(pos) => Self::new_local(value, pos),
        }
    }

    fn new_global(value: T) -> TypedScopedValue<T> {
        TypedScopedValue {
            global: Some(value),
            locals: vec![],
            forward_ids: vec![],
        }
    }

    fn new_local(value: T, pos: usize) -> TypedScopedValue<T> {
        let mut locals = Vec::with_capacity(pos);
        for _ in 0..pos {
            locals.push(None);
        }
        locals.push(Some(value));

        TypedScopedValue {
            global: None,
            locals,
            forward_ids: vec![],
        }
    }

    fn get(&self, id: ScopeId) -> Option<&T> {
        match id {
            ScopeId::Global => self.global.as_ref(),
            ScopeId::Local(pos) => self.get_local(pos),
        }
    }

    fn get_local(&self, pos: usize) -> Option<&T> {
        match *self.forward_ids.get(pos)? {
            Some(ScopeId::Local(id)) => self.locals.get(id)?.as_ref(),
            _ => self.global.as_ref(),
        }
    }

    fn set(&mut self, value: T, id: ScopeId) {
        match id {
            ScopeId::Global => self.global = Some(value),
            ScopeId::Local(pos) => self.set_local(value, pos),
        }
    }

    fn set_local(&mut self, value: T, pos: usize) {
        if self.locals.get_mut(pos).map_or(false, |v| v.is_some()) {
            return;
        }

        if self.locals.len() < pos {
            let len = pos - self.locals.len();
            self.locals.reserve_exact(len);
            for _ in 0..len {
                self.locals.push(None);
            }
        }
        self.locals.push(Some(value));
    }

    fn finalize(&mut self, parents: &[ScopeId]) {
        if parents.len() > self.locals.len() {
            let additional = parents.len() - self.locals.len();
            for _ in 0..additional {
                self.locals.push(None);
            }
        }

        self.forward_ids = {
            let lookup = |mut pos: usize| -> Option<ScopeId> {
                loop {
                    if self.locals[pos].is_some() {
                        return Some(ScopeId::Local(pos));
                    }
                    pos = parents.get(pos)?.local_id()?;
                }
            };

            (0..parents.len()).map(lookup).collect()
        };
    }
}

trait ScopedValue: Send + Sync + 'static {
    fn fmt_debug(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
    fn finalize(&mut self, parents: &[ScopeId]);
}

impl<T: Send + Sync + 'static> ScopedValue for TypedScopedValue<T> {
    fn fmt_debug(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }

    fn finalize(&mut self, parents: &[ScopeId]) {
        self.finalize(parents);
    }
}

impl fmt::Debug for dyn ScopedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_debug(f)
    }
}

impl dyn ScopedValue {
    #[cfg_attr(feature = "cargo-clippy", allow(cast_ptr_alignment))]
    unsafe fn downcast_ref_unchecked<T: Send + Sync + 'static>(&self) -> &TypedScopedValue<T> {
        &*(self as *const dyn ScopedValue as *const TypedScopedValue<T>)
    }

    #[cfg_attr(feature = "cargo-clippy", allow(cast_ptr_alignment))]
    unsafe fn downcast_mut_unchecked<T: Send + Sync + 'static>(
        &mut self,
    ) -> &mut TypedScopedValue<T> {
        &mut *(self as *mut dyn ScopedValue as *mut TypedScopedValue<T>)
    }
}

// ==== Container ====

struct IdentHash(u64);

impl Default for IdentHash {
    fn default() -> IdentHash {
        IdentHash(0)
    }
}

impl Hasher for IdentHash {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u8(*b);
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.0 = (self.0 << 8) | u64::from(i);
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}

#[derive(Debug)]
pub(crate) struct ScopedMap {
    map: HashMap<TypeId, Box<dyn ScopedValue>, BuildHasherDefault<IdentHash>>,
}

impl ScopedMap {
    pub(crate) fn get<T>(&self, id: ScopeId) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        unsafe {
            self.map
                .get(&TypeId::of::<T>())?
                .downcast_ref_unchecked()
                .get(id)
        }
    }
}

#[derive(Default)]
pub(super) struct Builder {
    map: HashMap<TypeId, Box<dyn ScopedValue>, BuildHasherDefault<IdentHash>>,
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder").finish()
    }
}

impl Builder {
    pub(super) fn set<T>(&mut self, value: T, id: ScopeId)
    where
        T: Send + Sync + 'static,
    {
        match self.map.entry(TypeId::of::<T>()) {
            Entry::Occupied(entry) => unsafe {
                entry.into_mut().downcast_mut_unchecked().set(value, id);
            },
            Entry::Vacant(entry) => {
                entry.insert(Box::new(TypedScopedValue::new(value, id)));
            }
        }
    }

    pub(super) fn finish(mut self, parents: &[ScopeId]) -> ScopedMap {
        for value in self.map.values_mut() {
            value.finalize(parents);
        }

        ScopedMap { map: self.map }
    }
}
