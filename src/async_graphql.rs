use serde::Deserialize;

fn invoke_handler<Runtime, Query, Mutation, Subscription>(
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
        .invoke_handler(invoke_handler(schema))
        .build()
}

#[cfg(feature = "graphql-ide")]
pub fn init_with_graphql_ide<Runtime, Query, Mutation, Subscription>(
    schema: async_graphql::Schema<Query, Mutation, Subscription>,
    graphql_ide_cfg: crate::GraphQlIdeConfig,
) -> tauri::plugin::TauriPlugin<Runtime>
where
    Runtime: tauri::Runtime,
    Query: async_graphql::ObjectType + 'static,
    Mutation: async_graphql::ObjectType + 'static,
    Subscription: async_graphql::SubscriptionType + 'static,
{
    use std::convert::Infallible;
    use warp::Filter;

    tauri::plugin::Builder::new("graphql-ipc")
        .invoke_handler(invoke_handler(schema.clone()))
        .setup({
            move |_app| {
                let graphql = async_graphql_warp::graphql(schema.clone()).and_then(
                    |(schema, request): (
                        async_graphql::Schema<Query, Mutation, Subscription>,
                        async_graphql::Request,
                    )| async move {
                        let response = schema.execute(request).await;
                        let response = async_graphql_warp::GraphQLResponse::from(response);
                        Ok::<_, Infallible>(response)
                    },
                );

                let graphql_ide = warp::path::end().and(warp::get()).map(move || {
                    let endpoint = graphql_ide_cfg.addr.to_string();
                    warp::http::Response::builder()
                        .header("Content-Type", "text/html")
                        .body(
                            async_graphql::http::GraphiQLSource::build()
                                .endpoint(&endpoint)
                                .finish(),
                        )
                });

                let routes = graphql_ide.or(graphql).recover(|err: warp::Rejection| async move {
                    if let Some(async_graphql_warp::GraphQLBadRequest(err)) = err.find() {
                        return Ok::<_, Infallible>(warp::reply::with_status(
                            err.to_string(),
                            http::StatusCode::BAD_REQUEST,
                        ));
                    }
                    Ok(warp::reply::with_status(
                        "INTERNAL_SERVER_ERROR".into(),
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                    ))
                });

                let server = warp::serve(routes).run(graphql_ide_cfg.addr);
                tauri::async_runtime::spawn(server);

                let graphql_ide_url = url::Url::parse(&format!("http://{}", graphql_ide_cfg.addr))?.to_string();
                println!("GraphiQL IDE: {}", graphql_ide_url);
                if graphql_ide_cfg.open {
                    open::that(graphql_ide_url)?;
                }

                Ok(())
            }
        })
        .build()
}

#[derive(Deserialize)]
pub struct GraphQLSubscriptionRequest {
    #[serde(flatten)]
    inner: async_graphql::Request,
    id: u32,
}
