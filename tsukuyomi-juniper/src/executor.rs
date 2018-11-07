//! GraphQL executor.

use std::sync::Arc;

use futures::{Async, Future, Poll};
use juniper::{GraphQLType, RootNode};

use tsukuyomi::error::Error;
use tsukuyomi::extractor::{Extract, Extractor};
use tsukuyomi::input::Input;

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
    pub fn execute(
        self,
        context: S::Context,
    ) -> impl Future<Item = GraphQLResponse, Error = Error> + Send + 'static {
        tsukuyomi::rt::blocking_section(move || {
            Ok::<_, tsukuyomi::error::Never>(self.request.execute(&*self.schema, &context))
        })
    }
}

/// Creates an `Extractor` which extracts an `Executor<S>`.
pub fn executor<S>(schema: S) -> ExecutorExtractor<S> {
    ExecutorExtractor {
        schema: Arc::new(schema),
        request: crate::request::request(),
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct ExecutorExtractor<S> {
    schema: Arc<S>,
    request: crate::request::GraphQLRequestExtractor,
}

impl<S> Extractor for ExecutorExtractor<S>
where
    S: Schema,
{
    type Output = (Executor<S>,);
    type Error = Error;
    type Future = ExecutorExtractorFuture<S>;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.request.extract(input)? {
            Extract::Ready((request,)) => Ok(Extract::Ready((Executor {
                request,
                schema: self.schema.clone(),
            },))),
            Extract::Incomplete(request) => Ok(Extract::Incomplete(ExecutorExtractorFuture {
                request,
                schema: Some(self.schema.clone()),
            })),
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct ExecutorExtractorFuture<S> {
    schema: Option<Arc<S>>,
    request: crate::request::GraphQLRequestExtractorFuture,
}

impl<S> Future for ExecutorExtractorFuture<S>
where
    S: Schema,
{
    type Item = (Executor<S>,);
    type Error = Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let (request,) = futures::try_ready!(self.request.poll());
        let schema = self.schema.take().unwrap();
        Ok(Async::Ready((Executor { schema, request },)))
    }
}
