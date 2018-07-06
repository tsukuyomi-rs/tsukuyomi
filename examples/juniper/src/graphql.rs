use juniper::{self, FieldResult};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, GraphQLEnum)]
pub enum Episode {
    NewHope,
    Empire,
    Jedi,
}

#[derive(Debug, Clone, GraphQLObject)]
#[graphql(description = "A humanoid creature in the Star Wars universe")]
pub struct Human {
    pub id: String,
    pub name: String,
    pub appears_in: Vec<Episode>,
    pub home_planet: String,
}

#[derive(Debug, GraphQLInputObject)]
#[graphql(description = "A humanoid creature in the Star Wars universe")]
pub struct NewHuman {
    name: String,
    appears_in: Vec<Episode>,
    home_planet: String,
}

/// Arbitrary context data.
#[derive(Debug, Default)]
pub struct Context(RwLock<Inner>);

#[derive(Debug, Default)]
struct Inner {
    humans: HashMap<u32, Human>,
    counter: u32,
}

impl juniper::Context for Context {}

impl Context {
    pub fn get_human(&self, id: String) -> FieldResult<Human> {
        let id: u32 = id.parse()?;
        let inner = self.0.read().map_err(|_| "failed to acquire a lock")?;
        inner.humans.get(&id).cloned().ok_or_else(|| "no such human".into())
    }

    pub fn all_humans(&self) -> FieldResult<Vec<Human>> {
        let inner = self.0.read().map_err(|_| "failed to acquire a lock")?;
        Ok(inner.humans.values().cloned().collect())
    }

    pub fn add_human(&self, new_human: NewHuman) -> FieldResult<Human> {
        let mut inner = self.0.write().map_err(|_| "failed to acquire a lock")?;

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

#[derive(Debug)]
pub struct Query {
    _priv: (),
}

graphql_object!(Query: Context |&self| {
    field apiVersion() -> &str {
        "1.0"
    }

    field human(&executor, id: String) -> FieldResult<Human> {
        executor.context().get_human(id)
    }

    field all_humans(&executor) -> FieldResult<Vec<Human>> {
        executor.context().all_humans()
    }
});

#[derive(Debug)]
pub struct Mutation {
    _priv: (),
}

graphql_object!(Mutation: Context |&self| {
    field create_human(&executor, new_human: NewHuman) -> FieldResult<Human> {
        executor.context().add_human(new_human)
    }
});

/// A root schema consists of a query and a mutation.
/// Request queries can be executed against a RootNode.
pub type Schema = juniper::RootNode<'static, Query, Mutation>;

pub fn create_schema() -> Schema {
    Schema::new(Query { _priv: () }, Mutation { _priv: () })
}
