#[cfg(feature = "juniper_subscriptions")]
use serde::Deserialize;
use std::sync::Arc;

pub struct Context<R: tauri::Runtime> {
    window: tauri::Window<R>,
}

impl<R: tauri::Runtime> Context<R> {
    pub fn window(&self) -> &tauri::Window<R> {
        &self.window
    }
}

impl<R: tauri::Runtime> juniper::Context for Context<R> {
}

fn handler<Runtime, Query, Mutation, Subscription, S>(
    schema: juniper::RootNode<'static, Query, Mutation, Subscription, S>,
) -> impl Fn(tauri::Invoke<Runtime>) + Send + Sync + 'static
where
    Runtime: tauri::Runtime,
    Query: juniper::GraphQLTypeAsync<S, Context = Context<Runtime>> + Send + 'static,
    Query::TypeInfo: Send + Sync,
    Mutation: juniper::GraphQLTypeAsync<S, Context = Context<Runtime>> + Send + 'static,
    Mutation::TypeInfo: Send + Sync,
    Subscription: juniper::GraphQLSubscriptionType<S, Context = Context<Runtime>> + Send + 'static,
    Subscription::TypeInfo: Send + Sync,
    S: juniper::ScalarValue + Send + Sync,
{
    let schema = Arc::new(schema);
    move |invoke| {
        let window = invoke.message.window();
        let context = Context { window: window.clone() };
        let schema = schema.clone();
        match invoke.message.command() {
            "graphql" => invoke.resolver.respond_async(async move {
                let payload = invoke.message.payload().clone();
                let request = serde_json::from_value::<juniper::http::GraphQLBatchRequest<S>>(payload)
                    .map_err(tauri::InvokeError::from_serde_json)?;
                let response = request
                    .execute::<Query, Mutation, Subscription>(&schema, &context)
                    .await;
                let serialized = serde_json::to_string(&response).map_err(tauri::InvokeError::from_serde_json)?;
                Ok((serialized, response.is_ok()))
            }),
            #[cfg(feature = "juniper_subscriptions")]
            "subscription" => invoke.resolver.respond_async(async move {
                use juniper::futures::{FutureExt, StreamExt, TryFutureExt};
                let request = {
                    let payload = invoke.message.payload().clone();
                    serde_json::from_value::<GraphQLSubscriptionRequest<S>>(payload)
                        .map_err(tauri::InvokeError::from_serde_json)?
                };
                let mut stream = juniper::http::resolve_into_stream(&request.inner, &schema, &context)
                    .map_ok(|(stream, errors)| juniper_subscriptions::Connection::from_stream(stream, errors))
                    .boxed()
                    .await?;
                let event = &format!("graphql://{}", request.id);
                while let Some(response) = stream.next().await {
                    let payload = serde_json::to_string(&response).map_err(tauri::InvokeError::from_serde_json)?;
                    window.emit(event, payload)?;
                }
                window.emit(event, None::<()>)?;
                Ok(())
            }),
            endpoint => invoke.resolver.reject(format!(
                r#"Invalid endpoint "{}". Valid endpoints are ["graphql", "subscription"]"#,
                endpoint
            )),
        }
    }
}

pub fn init<Runtime, Query, Mutation, Subscription, S>(
    schema: juniper::RootNode<'static, Query, Mutation, Subscription, S>,
) -> tauri::plugin::TauriPlugin<Runtime>
where
    Runtime: tauri::Runtime,
    Query: juniper::GraphQLTypeAsync<S, Context = Context<Runtime>> + Send + 'static,
    Query::TypeInfo: Send + Sync,
    Mutation: juniper::GraphQLTypeAsync<S, Context = Context<Runtime>> + Send + 'static,
    Mutation::TypeInfo: Send + Sync,
    Subscription: juniper::GraphQLSubscriptionType<S, Context = Context<Runtime>> + Send + 'static,
    Subscription::TypeInfo: Send + Sync,
    S: juniper::ScalarValue + Send + Sync + 'static,
{
    tauri::plugin::Builder::new("graphql-ipc")
        .invoke_handler(handler(schema))
        .build()
}

#[cfg(feature = "juniper_subscriptions")]
#[derive(Debug, Deserialize)]
pub struct GraphQLSubscriptionRequest<S: juniper::ScalarValue> {
    #[serde(flatten, bound(deserialize = "juniper::InputValue<S>: serde::Deserialize<'de>"))]
    inner: juniper::http::GraphQLRequest<S>,
    id: u32,
}
