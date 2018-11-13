//! GraphQL executor.

use std::sync::Arc;

use futures::{Async, Future};
use juniper::{GraphQLType, RootNode};

use tsukuyomi::error::Error;
use tsukuyomi::extractor::Extractor;

use crate::request::{GraphQLRequest, GraphQLResponse};

/// A marker trait representing a root node of GraphQL schema.
pub trait Schema: SchemaImpl {}

#[doc(hidden)]
pub trait SchemaImpl: Send + Sync + 'static {
    type Query: GraphQLType<Context = Self::Context, TypeInfo = Self::QueryTypeInfo>
        + Send
        + Sync
        + 'static;
    type QueryTypeInfo: Send + Sync + 'static;
    type Mutation: GraphQLType<Context = Self::Context, TypeInfo = Self::MutationTypeInfo>
        + Send
        + Sync
        + 'static;
    type MutationTypeInfo: Send + Sync + 'static;
    type Context: Send + 'static;

    fn as_root_node(&self) -> &RootNode<'static, Self::Query, Self::Mutation>;
}

impl<QueryT, MutationT, CtxT> Schema for RootNode<'static, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = CtxT> + Send + Sync + 'static,
    MutationT: GraphQLType<Context = CtxT> + Send + Sync + 'static,
    QueryT::TypeInfo: Send + Sync + 'static,
    MutationT::TypeInfo: Send + Sync + 'static,
    CtxT: Send + 'static,
{}

impl<QueryT, MutationT, CtxT> SchemaImpl for RootNode<'static, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = CtxT> + Send + Sync + 'static,
    MutationT: GraphQLType<Context = CtxT> + Send + Sync + 'static,
    QueryT::TypeInfo: Send + Sync + 'static,
    MutationT::TypeInfo: Send + Sync + 'static,
    CtxT: Send + 'static,
{
    type Query = QueryT;
    type QueryTypeInfo = QueryT::TypeInfo;
    type Mutation = MutationT;
    type MutationTypeInfo = MutationT::TypeInfo;
    type Context = CtxT;

    #[inline]
    fn as_root_node(&self) -> &RootNode<'static, Self::Query, Self::Mutation> {
        self
    }
}

/// GraphQL executor.
#[derive(Debug)]
pub struct Executor<S> {
    schema: Arc<S>,
    request: GraphQLRequest,
}

impl<S> Executor<S>
where
    S: Schema,
{
    /// Executes a GraphQL request from client with the specified context.
    pub fn execute<CtxT>(
        self,
        context: CtxT,
    ) -> impl Future<Item = GraphQLResponse, Error = Error> + Send + 'static
    where
        CtxT: AsRef<S::Context> + Send + 'static,
    {
        tsukuyomi::rt::blocking_section(move || {
            Ok::<_, tsukuyomi::error::Never>(self.request.execute(&*self.schema, context.as_ref()))
        })
    }
}

/// Creates an `Extractor` which extracts an `Executor<S>`.
pub fn executor<S>(schema: S) -> impl Extractor<Output = (Executor<S>,), Error = Error>
where
    S: Schema,
{
    let schema = Arc::new(schema);
    let request = crate::request::request();

    tsukuyomi::extractor::raw(move |input| {
        request.extract(input).map(|status| {
            status.map(
                |(request,)| {
                    (Executor {
                        schema: schema.clone(),
                        request,
                    },)
                },
                |mut future| {
                    let mut schema = Some(schema.clone());
                    futures::future::poll_fn(move || {
                        let (request,) = futures::try_ready!(future.poll());
                        let schema = schema.take().expect("The future has already polled.");
                        Ok(Async::Ready((Executor { schema, request },)))
                    })
                },
            )
        })
    })
}
