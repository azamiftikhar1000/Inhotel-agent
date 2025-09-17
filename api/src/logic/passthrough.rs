use super::get_connection;
use crate::{domain::metrics::Metric, server::AppState};
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use bson::doc;
use chrono::Utc;
use http::{header::CONTENT_LENGTH, HeaderMap, HeaderName, Method, Uri};
use hyper::body::Bytes;
use mongodb::options::FindOneOptions;
use osentities::{
    constant::PICA_PASSTHROUGH_HEADER,
    destination::{Action, Destination},
    encrypted_access_key::EncryptedAccessKey,
    event_access::EventAccess,
    prefix::IdPrefix,
    AccessKey, ApplicationError, Event, Id, InternalError, Store, META, PASSWORD_LENGTH,
    QUERY_BY_ID_PASSTHROUGH,
};
use serde::Deserialize;
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tracing::{error, info};
use unified::domain::UnifiedMetadataBuilder;

pub fn get_router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/*key",
        get(passthrough_request)
            .post(passthrough_request)
            .patch(passthrough_request)
            .put(passthrough_request)
            .delete(passthrough_request),
    )
}

pub async fn passthrough_request(
    Extension(user_event_access): Extension<Arc<EventAccess>>,
    State(state): State<Arc<AppState>>,
    mut headers: HeaderMap,
    query_params: Option<Query<HashMap<String, String>>>,
    uri: Uri,
    method: Method,
    body: Bytes,
) -> impl IntoResponse {
    println!("ENTRY POINT: passthrough_request function entered");
    println!("ENTRY POINT: URI: {}", uri);
    println!("ENTRY POINT: Method: {}", method);
    println!("ENTRY POINT: Headers count: {}", headers.len());
    
    // Log all headers (sanitized)
    for (key, value) in headers.iter() {
        let display_value = if key.as_str().to_lowercase().contains("auth") 
            || key.as_str().to_lowercase().contains("key") 
            || key.as_str().to_lowercase().contains("secret") {
            "[REDACTED]".to_string()
        } else {
            value.to_str().unwrap_or("[BINARY]").to_string()
        };
        println!("ENTRY POINT: Header - {}: {}", key, display_value);
    }
    
    // Log expected header names from config
    println!("ENTRY POINT: Expected connection header name: {}", state.config.headers.connection_header);
    println!("ENTRY POINT: Expected auth header name: {}", state.config.headers.auth_header);
    
    let Some(connection_key_header) = headers.get(&state.config.headers.connection_header) else {
        println!("ERROR: Connection header not found. Expected header name: {}", state.config.headers.connection_header);
        return Err(ApplicationError::bad_request(
            "Connection header not found",
            None,
        ));
    };
    println!("CHECKPOINT 1: Connection header found");

    let Some(connection_secret_header) = headers.get(&state.config.headers.auth_header) else {
        println!("ERROR: Auth header not found. Expected header name: {}", state.config.headers.auth_header);
        return Err(ApplicationError::bad_request(
            "Connection header not found",
            None,
        ));
    };
    println!("CHECKPOINT 2: Auth header found");

    let host = headers.get("host");
    let host = host.and_then(|h| h.to_str().map(|s| s.to_string()).ok());
    println!("CHECKPOINT 3: Host processed: {:?}", host);

    let connection_secret_header = connection_secret_header.clone();

    println!("CHECKPOINT 4: About to call get_connection");
    let connection = get_connection(
        user_event_access.as_ref(),
        connection_key_header,
        &state.app_stores,
        &state.connections_cache,
    )
    .await
    .map_err(|e| {
        println!("ERROR: Failed to get connection: {}", e);
        e
    })?;
    println!("CHECKPOINT 5: Connection retrieved successfully: {}", connection.id);

    let id_header = headers.get(QUERY_BY_ID_PASSTHROUGH);
    let id = id_header.and_then(|h| h.to_str().ok());
    let id_str = id.map(|i| i.to_string());
    println!("CHECKPOINT 6: ID header processed: {:?}", id_str);

    info!("Executing {} request on {}", method, uri.path());

    let destination = Destination {
        platform: connection.platform.clone(),
        action: Action::Passthrough {
            path: uri.path().into(),
            method: method.clone(),
            id: id.map(|i| i.into()),
        },
        connection_key: connection.key.clone(),
    };
    println!("CHECKPOINT 7: Destination created");

    let Query(query_params) = query_params.unwrap_or_default();
    println!("CHECKPOINT 8: Query params processed: {:?}", query_params);

    headers.remove(&state.config.headers.auth_header);
    headers.remove(&state.config.headers.connection_header);
    println!("CHECKPOINT 9: Auth and connection headers removed");

    // Add debugging logs before making the request
    println!("DIAGNOSTIC PASSTHROUGH: About to dispatch request");
    println!("DIAGNOSTIC PASSTHROUGH: Connection ID: {}", connection.id);
    println!("DIAGNOSTIC PASSTHROUGH: Platform: {}", connection.platform);
    println!("DIAGNOSTIC PASSTHROUGH: URI Path: {}", uri.path());
    println!("DIAGNOSTIC PASSTHROUGH: Method: {}", method);
    println!("DIAGNOSTIC PASSTHROUGH: Headers count: {}", headers.len());
    println!("DIAGNOSTIC PASSTHROUGH: Query params: {:?}", query_params);

    // If there's an ID, log it
    if let Some(id_val) = id_str.as_ref() {
        println!("DIAGNOSTIC PASSTHROUGH: ID from header: {}", id_val);
    }

    // Now make the request
    println!("CHECKPOINT 10: About to call dispatch_destination_request");
    let model_execution_result = state
        .extractor_caller
        .dispatch_destination_request(
            Some(connection.clone()),
            &destination,
            headers.clone(),
            query_params,
            Some(body.to_vec()),
        )
        .await
        .map_err(|e| {
            // Log more details about the error
            println!("DIAGNOSTIC PASSTHROUGH ERROR: {}", e);
            println!("DIAGNOSTIC PASSTHROUGH ERROR TYPE: {}", std::any::type_name_of_val(&e));
            
            error!("Failed to execute connection model definition in passthrough endpoint. ID: {}, Error: {}", connection.id, e);

            e
        })?;
    println!("CHECKPOINT 11: dispatch_destination_request completed successfully");

    let mut headers = HeaderMap::new();

    model_execution_result
        .headers()
        .into_iter()
        .for_each(|(key, value)| match key {
            &CONTENT_LENGTH => {
                headers.insert(CONTENT_LENGTH, value.clone());
            }
            _ => {
                if let Ok(header_name) =
                    HeaderName::try_from(format!("{PICA_PASSTHROUGH_HEADER}-{key}"))
                {
                    headers.insert(header_name, value.clone());
                };
            }
        });
    println!("CHECKPOINT 12: Response headers processed");

    let connection_platform = connection.platform.to_string();
    let connection_platform_version = connection.platform_version.to_string();
    let connection_key = connection.key.to_string();
    let request_headers = headers.clone();
    let request_status_code = model_execution_result.status();
    println!("CHECKPOINT 13: Response status code: {}", request_status_code);

    let database_c = state.app_stores.db.clone();
    let event_access_pass_c = state.config.event_access_password.clone();
    let event_tx_c = state.event_tx.clone();

    tokio::spawn(async move {
        let connection_secret_header: Option<String> =
            connection_secret_header.to_str().map(|a| a.to_owned()).ok();

        let options = FindOneOptions::builder()
            .projection(doc! {
                "connectionPlatform": 1,
                "connectionDefinitionId": 1,
                "platformVersion": 1,
                "key": 1,
                "title": 1,
                "name": 1,
                "path": 1,
                "action": 1,
                "actionName": 1
            })
            .build();

        let query = if let Some(id) = id_str {
            doc! {
                "_id": id.to_string(),
            }
        } else {
            doc! {
                "connectionPlatform": connection_platform.clone(),
                "path": uri.path().to_string(),
                "action": method.to_string().to_uppercase()
            }
        };

        let cmd = database_c
            .collection::<SparseCMD>(&Store::ConnectionModelDefinitions.to_string())
            .find_one(query)
            .with_options(options.clone())
            .await
            .ok()
            .flatten();

        if let (Some(cmd), Some(encrypted_access_key)) = (cmd, connection_secret_header) {
            if let Ok(encrypted_access_key) = EncryptedAccessKey::parse(&encrypted_access_key) {
                let path = uri.path().trim_end_matches('/');

                let metadata = UnifiedMetadataBuilder::default()
                    .timestamp(Utc::now().timestamp_millis())
                    .platform_rate_limit_remaining(0)
                    .rate_limit_remaining(0)
                    .transaction_key(Id::now(IdPrefix::Transaction))
                    .platform(connection_platform.clone())
                    .platform_version(connection_platform_version.clone())
                    .common_model_version("v1")
                    .connection_key(connection_key)
                    .action(cmd.title)
                    .host(host)
                    .path(path.to_string())
                    .status_code(request_status_code)
                    .build()
                    .ok()
                    .map(|m| m.as_value());

                let password: Option<[u8; PASSWORD_LENGTH]> =
                    event_access_pass_c.as_bytes().try_into().ok();

                match password {
                    Some(password) => {
                        let access_key = AccessKey::parse(&encrypted_access_key, &password).ok();

                        let event_name = format!(
                            "{}::{}::{}::{}",
                            connection_platform,
                            connection_platform_version,
                            cmd.name,
                            cmd.action_name
                        );

                        let name = if request_status_code.is_success() {
                            format!("{event_name}::request-succeeded",)
                        } else {
                            format!("{event_name}::request-failed",)
                        };

                        let body = serde_json::to_string(&json!({
                            META: metadata,
                        }))
                        .unwrap_or_default();

                        if let Some(access_key) = access_key {
                            let event = Event::new(
                                &access_key,
                                &encrypted_access_key,
                                &name,
                                request_headers.clone(),
                                body,
                            );

                            if let Err(e) = event_tx_c.send(event).await {
                                error!("Could not send event to receiver: {e}");
                            }
                        }
                    }
                    None => {
                        // Handle error silently
                    }
                };
            }
        };
    });

    let metric = Metric::passthrough(connection);
    if let Err(e) = state.metric_tx.send(metric).await {
        error!("Could not send metric to receiver: {e}");
    }
    println!("CHECKPOINT 14: Metric sent");

    let bytes = model_execution_result.bytes().await.map_err(|e| {
        error!(
            "Error retrieving bytes from response in passthrough endpoint: {:?}",
            e
        );
        println!("ERROR: Failed to retrieve bytes from response: {:?}", e);
        InternalError::script_error("Error retrieving bytes from response", None)
    })?;
    println!("CHECKPOINT 15: Response bytes retrieved");

    println!("CHECKPOINT 16: Returning response with status: {}", request_status_code);
    Ok((request_status_code, headers, bytes))
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SparseCMD {
    pub connection_platform: String,
    pub connection_definition_id: Id,
    pub platform_version: String,
    pub key: String,
    pub title: String,
    pub name: String,
    pub path: String,
    #[serde(with = "http_serde_ext_ios::method")]
    pub action: Method,
    pub action_name: String,
}
