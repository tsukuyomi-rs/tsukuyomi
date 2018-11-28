use {
    crate::schema::{Human, NewHuman},
    juniper::FieldResult,
    std::{
        collections::HashMap,
        sync::{Arc, RwLock},
    },
};

#[derive(Debug, Default)]
pub struct Database {
    humans: HashMap<u32, Human>,
    counter: u32,
}

/// Arbitrary context data.
#[derive(Debug, Default)]
pub struct Context {
    pub database: Arc<RwLock<Database>>,
}

impl juniper::Context for Context {}

impl Context {
    pub fn get_human(&self, id: &str) -> FieldResult<Human> {
        let id: u32 = id.parse()?;
        let inner = self
            .database
            .read()
            .map_err(|_| "failed to acquire a lock")?;
        inner
            .humans
            .get(&id)
            .cloned()
            .ok_or_else(|| "no such human".into())
    }

    pub fn all_humans(&self) -> FieldResult<Vec<Human>> {
        let inner = self
            .database
            .read()
            .map_err(|_| "failed to acquire a lock")?;
        Ok(inner.humans.values().cloned().collect())
    }

    pub fn add_human(&self, new_human: NewHuman) -> FieldResult<Human> {
        let mut inner = self
            .database
            .write()
            .map_err(|_| "failed to acquire a lock")?;

        let new_id = inner.counter;

        let human = Human {
            id: new_id.to_string(),
            name: new_human.name,
            appears_in: new_human.appears_in,
            home_planet: new_human.home_planet,
        };

        inner.humans.insert(new_id, human.clone());
        inner.counter += 1;

        Ok(human)
    }
}
