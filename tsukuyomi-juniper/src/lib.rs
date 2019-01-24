//! Components for integrating GraphQL endpoints into Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-juniper/0.4.0-dev")]
#![deny(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

mod error;
mod graphiql;
mod request;

pub use crate::{
    error::{capture_errors, CaptureErrors},
    graphiql::graphiql_source,
    request::{request, GraphQLRequest, GraphQLResponse},
};

use {
    juniper::{DefaultScalarValue, GraphQLType, RootNode, ScalarRefValue, ScalarValue},
    std::sync::Arc,
};

/// A marker trait representing a root node of GraphQL schema.
#[allow(missing_docs)]
pub trait Schema<S = DefaultScalarValue>
where
    S: ScalarValue,
    for<'a> &'a S: ScalarRefValue<'a>,
{
    type Query: GraphQLType<S, Context = Self::Context, TypeInfo = Self::QueryInfo>;
    type QueryInfo;
    type Mutation: GraphQLType<S, Context = Self::Context, TypeInfo = Self::MutationInfo>;
    type MutationInfo;
    type Context;

    fn as_root_node(&self) -> &RootNode<'static, Self::Query, Self::Mutation, S>;
}

impl<QueryT, MutationT, CtxT, S> Schema<S> for RootNode<'static, QueryT, MutationT, S>
where
    QueryT: GraphQLType<S, Context = CtxT>,
    MutationT: GraphQLType<S, Context = CtxT>,
    S: ScalarValue,
    for<'a> &'a S: ScalarRefValue<'a>,
{
    type Query = QueryT;
    type QueryInfo = QueryT::TypeInfo;
    type Mutation = MutationT;
    type MutationInfo = MutationT::TypeInfo;
    type Context = CtxT;

    #[inline]
    fn as_root_node(&self) -> &RootNode<'static, Self::Query, Self::Mutation, S> {
        self
    }
}

impl<T, S> Schema<S> for Box<T>
where
    T: Schema<S>,
    S: ScalarValue,
    for<'a> &'a S: ScalarRefValue<'a>,
{
    type Query = T::Query;
    type QueryInfo = T::QueryInfo;
    type Mutation = T::Mutation;
    type MutationInfo = T::MutationInfo;
    type Context = T::Context;

    #[inline]
    fn as_root_node(&self) -> &RootNode<'static, Self::Query, Self::Mutation, S> {
        (**self).as_root_node()
    }
}

impl<T, S> Schema<S> for Arc<T>
where
    T: Schema<S>,
    S: ScalarValue,
    for<'a> &'a S: ScalarRefValue<'a>,
{
    type Query = T::Query;
    type QueryInfo = T::QueryInfo;
    type Mutation = T::Mutation;
    type MutationInfo = T::MutationInfo;
    type Context = T::Context;

    #[inline]
    fn as_root_node(&self) -> &RootNode<'static, Self::Query, Self::Mutation, S> {
        (**self).as_root_node()
    }
}
