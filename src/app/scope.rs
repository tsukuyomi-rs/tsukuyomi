use std::any::TypeId;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::{fmt, mem};

// ==== ScopedValue ====

pub(super) struct TypedScopedValue<T> {
    pub(super) global: Option<T>,
    pub(super) locals: Vec<Option<T>>,
    pub(super) forward_ids: Vec<Option<usize>>,
}

impl<T> fmt::Debug for TypedScopedValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let locals = self.locals.iter().map(|_| "<value>").collect::<Vec<_>>();
        f.debug_struct("TypedScopedValue")
            .field("global", &self.global.as_ref().map(|_| "<value>"))
            .field("locals", &locals)
            .field("forward_ids", &self.forward_ids)
            .finish()
    }
}

impl<T> TypedScopedValue<T> {
    fn new(value: T, scope_id: Option<usize>) -> TypedScopedValue<T> {
        if let Some(scope_id) = scope_id {
            let mut locals = Vec::with_capacity(scope_id);
            for _ in 0..scope_id {
                locals.push(None);
            }
            locals.push(Some(value));

            TypedScopedValue {
                global: None,
                locals: locals,
                forward_ids: vec![],
            }
        } else {
            TypedScopedValue {
                global: Some(value),
                locals: vec![],
                forward_ids: vec![],
            }
        }
    }

    fn get_forward(&self, scope_id: usize) -> Option<&T> {
        let forward_id = (*self.forward_ids.get(scope_id)?)?;
        self.get_local(forward_id)
    }

    fn get_local(&self, scope_id: usize) -> Option<&T> {
        self.locals.get(scope_id)?.as_ref()
    }

    fn get_global(&self) -> Option<&T> {
        self.global.as_ref()
    }

    fn set(&mut self, value: T, scope_id: Option<usize>) {
        if let Some(scope_id) = scope_id {
            if self.locals.get_mut(scope_id).map_or(false, |v| v.is_some()) {
                return;
            }

            if self.locals.len() < scope_id {
                let len = scope_id - self.locals.len();
                self.locals.reserve_exact(len);
                for _ in 0..len {
                    self.locals.push(None);
                }
            }
            self.locals.push(Some(value));
        } else {
            self.global.get_or_insert(value);
        }
    }

    fn finalize(&mut self, parents: &[Option<usize>]) {
        if parents.len() > self.locals.len() {
            let additional = parents.len() - self.locals.len();
            for _ in 0..additional {
                self.locals.push(None);
            }
        }

        self.forward_ids = {
            let lookup = |mut pos: usize| -> Option<usize> {
                loop {
                    if self.locals[pos].is_some() {
                        return Some(pos);
                    }
                    pos = (*parents.get(pos)?)?;
                }
            };

            (0..parents.len()).map(lookup).collect()
        };
    }
}

trait Sealed {}
impl<T: 'static> Sealed for TypedScopedValue<T> {}

trait ScopedValue: Sealed {
    #[doc(hidden)]
    fn fmt_debug(&self, f: &mut fmt::Formatter) -> fmt::Result;
    #[doc(hidden)]
    fn type_id(&self) -> TypeId;

    fn finalize(&mut self, parents: &[Option<usize>]);
}

impl<T: 'static> ScopedValue for TypedScopedValue<T> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn fmt_debug(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }

    fn finalize(&mut self, parents: &[Option<usize>]) {
        self.finalize(parents);
    }
}

impl fmt::Debug for dyn ScopedValue + Send + Sync + 'static {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_debug(f)
    }
}

impl dyn ScopedValue + Send + Sync + 'static {
    fn is<T: Send + Sync + 'static>(&self) -> bool {
        TypeId::of::<T>() == self.type_id()
    }

    fn downcast_ref<T: Send + Sync + 'static>(&self) -> Option<&TypedScopedValue<T>> {
        if self.is::<T>() {
            unsafe { Some(&*(self as *const dyn ScopedValue as *const TypedScopedValue<T>)) }
        } else {
            None
        }
    }

    fn downcast_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut TypedScopedValue<T>> {
        if self.is::<T>() {
            unsafe { Some(&mut *(self as *mut dyn ScopedValue as *mut TypedScopedValue<T>)) }
        } else {
            None
        }
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
        self.0 = (self.0 << 8) | (i as u64);
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}

#[derive(Debug)]
pub(crate) struct Container {
    map: HashMap<TypeId, Box<dyn ScopedValue + Send + Sync + 'static>, BuildHasherDefault<IdentHash>>,
}

impl Container {
    pub(super) fn builder() -> Builder {
        Builder::new()
    }

    pub(crate) fn get<T>(&self, scope_id: usize) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        let inner = self.get_inner()?;
        inner.get_forward(scope_id).or_else(|| inner.get_global())
    }

    /*
    pub(super) fn get_local<T>(&self, scope_id: usize) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.get_inner()?.get_local(scope_id)
    }

    pub(super) fn get_global<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.get_inner()?.get_global()
    }
    */

    pub(super) fn get_inner<T: Send + Sync + 'static>(&self) -> Option<&TypedScopedValue<T>> {
        self.map.get(&TypeId::of::<T>())?.downcast_ref()
    }
}

pub(super) struct Builder {
    map: HashMap<TypeId, Box<dyn ScopedValue + Send + Sync + 'static>, BuildHasherDefault<IdentHash>>,
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Builder").finish()
    }
}

impl Builder {
    fn new() -> Self {
        Builder {
            map: HashMap::with_hasher(Default::default()),
        }
    }

    pub(super) fn set<T>(&mut self, value: T, scope_id: Option<usize>)
    where
        T: Send + Sync + 'static,
    {
        let mut value_opt = Some(value);
        self.map
            .entry(TypeId::of::<T>())
            .and_modify(|scoped_value| {
                let inner = scoped_value.downcast_mut().expect("type mismatch");
                inner.set(value_opt.take().unwrap(), scope_id);
            })
            .or_insert_with(|| Box::new(TypedScopedValue::new(value_opt.take().unwrap(), scope_id)));
    }

    pub(super) fn finish(&mut self, parents: &[Option<usize>]) -> Container {
        let Builder { mut map } = mem::replace(self, Builder::new());

        for value in map.values_mut() {
            value.finalize(parents);
        }

        Container { map }
    }
}
