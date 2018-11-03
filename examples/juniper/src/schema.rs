use juniper::{self, FieldResult};

use context::Context;

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
    pub name: String,
    pub appears_in: Vec<Episode>,
    pub home_planet: String,
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
