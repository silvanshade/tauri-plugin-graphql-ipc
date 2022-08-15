use serde::Deserialize;

fn handler<Runtime, Query, Mutation, Subscription>(
    schema: async_graphql::Schema<Query, Mutation, Subscription>,
) -> impl Fn(tauri::Invoke<Runtime>) + Send + Sync + 'static
where
    Runtime: tauri::Runtime,
    Query: async_graphql::ObjectType + 'static,
    Mutation: async_graphql::ObjectType + 'static,
    Subscription: async_graphql::SubscriptionType + 'static,
{
    move |invoke| {
        let window = invoke.message.window();
        let schema = schema.clone();
        match invoke.message.command() {
            "graphql" => invoke.resolver.respond_async(async move {
                let payload = invoke.message.payload().clone();
                let request = serde_json::from_value::<async_graphql::BatchRequest>(payload)
                    .map_err(tauri::InvokeError::from_serde_json)?
                    .data(window);
                let response = schema.execute_batch(request).await;
                let serialized = serde_json::to_string(&response).map_err(tauri::InvokeError::from_serde_json)?;
                Ok((serialized, response.is_ok()))
            }),
            "subscription" => invoke.resolver.respond_async(async move {
                use async_graphql::futures_util::StreamExt;
                let request = {
                    let payload = invoke.message.payload().clone();
                    serde_json::from_value::<GraphQLSubscriptionRequest>(payload)
                        .map_err(tauri::InvokeError::from_serde_json)?
                };
                let mut stream = {
                    let request = request.inner.data(window.clone());
                    schema.execute_stream(request)
                };
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

pub fn init<Runtime, Query, Mutation, Subscription>(
    schema: async_graphql::Schema<Query, Mutation, Subscription>,
) -> tauri::plugin::TauriPlugin<Runtime>
where
    Runtime: tauri::Runtime,
    Query: async_graphql::ObjectType + 'static,
    Mutation: async_graphql::ObjectType + 'static,
    Subscription: async_graphql::SubscriptionType + 'static,
{
    tauri::plugin::Builder::new("graphql-ipc")
        .invoke_handler(handler(schema))
        .build()
}

#[derive(Deserialize)]
pub struct GraphQLSubscriptionRequest {
    #[serde(flatten)]
    inner: async_graphql::Request,
    id: u32,
}
