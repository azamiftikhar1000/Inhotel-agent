use super::{
    create, delete, read, update, HookExt, PublicExt, ReadResponse, RequestExt, SuccessResponse,
};
use crate::{
    helper::shape_mongo_filter,
    router::ServerResponse,
    server::{AppState, AppStores},
};
use axum::{
    extract::Query,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{patch, post},
    Extension, Json, Router,
};
use chrono::Utc;
use fake::Dummy;
use mongodb::bson::doc;
use osentities::{
    algebra::MongoStore,
    api_model_config::{
        ApiModelConfig, AuthMethod, ModelPaths, ResponseBody, SamplesInput, SchemasInput,
    },
    connection_definition::ConnectionDefinition,
    connection_model_definition::{
        ConnectionModelDefinition, CrudAction, CrudMapping, ExtractorConfig, PlatformInfo,
        TestConnection, TestConnectionState,
    },
    connection_model_schema::ConnectionModelSchema,
    connection_oauth_definition::Settings,
    event_access::EventAccess,
    id::{prefix::IdPrefix, Id},
    platform::PlatformData,
    ApplicationError, InternalError, PicaError,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio::try_join;
use tracing::error;

pub fn get_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/",
            post(create::<CreateRequest, ConnectionModelDefinition>)
                .get(read::<CreateRequest, ConnectionModelDefinition>)
                .patch(update_many),
        )
        .route(
            "/:id",
            patch(update::<CreateRequest, ConnectionModelDefinition>)
                .delete(delete::<CreateRequest, ConnectionModelDefinition>),
        )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchUpdateResult {
    pub id: Option<String>,
    pub success: bool,
    pub error: Option<String>,
}


#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialUpdateRequest {
    #[serde(rename = "_id")]
    pub id: Option<Id>,
    pub connection_platform: Option<String>,
    pub connection_definition_id: Option<Id>,
    pub platform_version: Option<String>,
    pub title: Option<String>,
    pub name: Option<String>,
    pub model_name: Option<String>,
    pub base_url: Option<String>,
    pub path: Option<String>,
    pub auth_method: Option<AuthMethod>,
    pub action_name: Option<CrudAction>,
    #[serde(with = "http_serde_ext_ios::method::option", rename = "action", default)]
    pub http_method: Option<http::Method>,
    #[serde(
        with = "http_serde_ext_ios::header_map::option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub headers: Option<HeaderMap>,
    pub query_params: Option<BTreeMap<String, String>>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub extractor_config: Option<ExtractorConfig>,
    pub schemas: Option<SchemasInput>,
    pub samples: Option<SamplesInput>,
    pub responses: Option<Vec<ResponseBody>>,
    pub version: Option<Version>,
    pub is_default_crud_mapping: Option<bool>,
    pub test_connection_payload: Option<Value>,
    pub test_connection_status: Option<TestConnection>,
    pub mapping: Option<CrudMapping>,
    pub paths: Option<ModelPaths>,
    pub supported: Option<bool>,
    pub active: Option<bool>,
    pub knowledge: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub async fn update_many(
    access: Option<Extension<Arc<EventAccess>>>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Vec<PartialUpdateRequest>>,
) -> Result<Json<ServerResponse<Vec<BatchUpdateResult>>>, PicaError> {
    let mut results = Vec::new();

    for request in payload {
        let id_str = match &request.id {
            Some(id) => id.to_string(),
            None => {
                results.push(BatchUpdateResult {
                    id: None,
                    success: false,
                    error: Some("Missing ID".to_string()),
                });
                continue;
            }
        };

        let mut query = shape_mongo_filter(
            None,
            access.clone().map(|e| {
                let Extension(e) = e;
                e
            }),
            None,
        );
        query.filter.insert("_id", &id_str);

        let store = CreateRequest::get_store(state.app_stores.clone());

        match store.get_one(query.filter).await {
            Ok(Some(mut record)) => {
                // Merging Logic
                if let Some(val) = request.connection_platform { record.connection_platform = val; }
                if let Some(val) = request.connection_definition_id { record.connection_definition_id = val; }
                if let Some(val) = request.platform_version { record.platform_version = val; }
                if let Some(val) = request.title { record.title = val; }
                if let Some(val) = request.name { record.name = val; }
                if let Some(val) = request.model_name { record.model_name = val; }
                if let Some(val) = request.action_name { record.action_name = val; }
                if let Some(val) = request.http_method { record.action = val; }
                
                // ApiModelConfig Merge
                if let PlatformInfo::Api(ref mut api_config) = record.platform_info {
                    if let Some(val) = request.base_url { api_config.base_url = val; }
                    if let Some(val) = request.path { api_config.path = val; }
                    if let Some(val) = request.auth_method { api_config.auth_method = val; }
                    if let Some(val) = request.headers { api_config.headers = Some(val); }
                    if let Some(val) = request.query_params { api_config.query_params = Some(val); }
                    if let Some(val) = request.schemas { api_config.schemas = val; }
                    if let Some(val) = request.samples { api_config.samples = val; }
                    if let Some(val) = request.responses { api_config.responses = val; }
                    if let Some(val) = request.paths { api_config.paths = Some(val); }
                }

                if let Some(val) = request.extractor_config { record.extractor_config = Some(val); }
                if let Some(val) = request.version { record.record_metadata.version = val; }
                if let Some(val) = request.is_default_crud_mapping { record.is_default_crud_mapping = Some(val); }
                if let Some(val) = request.test_connection_payload { record.test_connection_payload = Some(val); }
                if let Some(val) = request.test_connection_status { record.test_connection_status = val; }
                if let Some(val) = request.mapping { record.mapping = Some(val); }
                if let Some(val) = request.supported { record.supported = val; }
                if let Some(val) = request.active { record.record_metadata.active = val; }
                if let Some(val) = request.knowledge { record.knowledge = Some(val); }
                if let Some(val) = request.tags { record.record_metadata.tags = val; }

                // Regenerate Key (Same logic as RequestExt)
                // Note: If fields involved in key generation didn't change, this stays same, 
                // but we regenerate to be safe if any one of them changed.
                 // Key generation relies on: connection_platform, platform_version, model_name, action_name, path, name
                 // We need 'path' which is inside api_config.
                let path_val = if let PlatformInfo::Api(ref api_config) = record.platform_info {
                    api_config.path.clone()
                } else {
                    String::new()
                };

                let key = format!(
                    "api::{}::{}::{}::{}::{}::{}",
                    record.connection_platform,
                    record.platform_version,
                    record.model_name,
                    record.action_name,
                    path_val,
                    record.name
                ).to_lowercase();
                record.key = key;

                let bson_result = bson::to_bson_with_options(&record, Default::default());

                match bson_result {
                    Ok(bson) => {
                        let document = doc! { "$set": bson };
                        match store.update_one(&id_str, document).await {
                            Ok(_) => {
                            CreateRequest::after_update_hook(&record, &state.app_stores)
                                    .await
                                    .ok();
                                results.push(BatchUpdateResult {
                                    id: Some(id_str),
                                    success: true,
                                    error: None,
                                });
                            }
                            Err(e) => {
                                error!("Error updating in store: {e}");
                                results.push(BatchUpdateResult {
                                    id: Some(id_str),
                                    success: false,
                                    error: Some(e.to_string()),
                                });
                            }
                        }
                    }
                    Err(e) => {
                         error!("Could not serialize record into document: {e}");
                         results.push(BatchUpdateResult {
                            id: Some(id_str),
                            success: false,
                            error: Some(e.to_string()),
                        });
                    }
                }
            }
            Ok(None) => {
                results.push(BatchUpdateResult {
                    id: Some(id_str),
                    success: false,
                    error: Some("Record not found".to_string()),
                });
            }
            Err(e) => {
                error!("Error getting record in store: {e}");
                 results.push(BatchUpdateResult {
                    id: Some(id_str),
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    Ok(Json(ServerResponse::new("batch_update", results)))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionPayload {
    pub connection_key: String,
    pub request: TestConnectionRequest,
}

#[derive(Debug, Clone, Deserialize, Serialize, Dummy)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionRequest {
    #[serde(
        with = "http_serde_ext_ios::header_map::option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub headers: Option<HeaderMap>,
    pub query_params: Option<HashMap<String, String>>,
    pub path_params: Option<HashMap<String, String>>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestConnectionResponse {
    #[serde(with = "http_serde_ext_ios::status_code")]
    pub code: StatusCode,
    pub status: TestConnection,
    pub meta: Meta,
    pub response: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub timestamp: u64,
    pub platform: String,
    pub platform_version: String,
    pub connection_definition_id: Id,
    pub connection_key: String,
    pub model_name: String,
    #[serde(with = "http_serde_ext_ios::method")]
    pub action: http::Method,
}

pub async fn test_connection_model_definition(
    Extension(access): Extension<Arc<EventAccess>>,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TestConnectionPayload>,
) -> Result<Json<ServerResponse<TestConnectionResponse>>, PicaError> {
    let connection = match state
        .app_stores
        .connection
        .get_one(doc! {
            "key": &payload.connection_key,
            "ownership.buildableId": access.ownership.id.as_ref(),
            "deleted": false
        })
        .await
    {
        Ok(Some(data)) => data,
        Ok(None) => {
            return Err(ApplicationError::not_found(
                &format!("Connection with key {} not found", payload.connection_key),
                None,
            ));
        }
        Err(e) => {
            error!("Error fetching connection in testing endpoint: {:?}", e);

            return Err(e);
        }
    };

    let connection_model_definition = match state
        .app_stores
        .model_config
        .get_one(doc! {
            "_id": id,
            "active": false, // Cannot test an active model definition
            "deleted": false
        })
        .await
    {
        Ok(Some(data)) => data,
        Ok(None) => {
            return Err(ApplicationError::not_found(
                "Inactive Connection Model Definition Record",
                None,
            ));
        }
        Err(e) => {
            error!(
                "Error fetching inactive connection model definition in testing endpoint: {:?}",
                e
            );

            return Err(e);
        }
    };

    let secret_result = state
        .secrets_client
        .get(&connection.secrets_service_id, &connection.ownership.id)
        .await
        .map_err(|e| {
            error!("Error decripting secret for connection: {:?}", e);

            e
        })?;

    let mut secret_result = secret_result.as_value()?;

    let request_string: String = serde_json::to_string(&payload.request.clone()).map_err(|e| {
        error!(
            "Error converting request to json string in testing endpoint: {:?}",
            e
        );

        InternalError::script_error("Could not serialize request payload", None)
    })?;

    // Add path params to template context
    if let Some(path_params) = payload.request.path_params {
        for (key, val) in path_params {
            secret_result[key] = Value::String(val);
        }
    }

    let request_body_vec = payload
        .request
        .body
        .map(|body| body.to_string().into_bytes());
    let model_execution_result = state
        .extractor_caller
        .execute_model_definition(
            &Arc::new(connection_model_definition.clone()),
            payload.request.headers.unwrap_or_default(),
            &payload.request.query_params.unwrap_or(HashMap::new()),
            &Arc::new(secret_result),
            request_body_vec,
        )
        .await
        .map_err(|e| {
            error!("Error executing connection model definition: {:?}", e);

            e
        })?;

    let status_code = model_execution_result.status();

    let response_body = model_execution_result.text().await.map_err(|e| {
        error!("Could not get text from test connection failure: {e}");

        InternalError::unknown("Could not get text from test connection", None)
    })?;

    let status = match status_code {
        status if status.is_success() => TestConnection {
            last_tested_at: Utc::now().timestamp_millis(),
            state: TestConnectionState::Success {
                response: response_body.clone(),
                request_payload: request_string,
            },
        },
        _ => TestConnection {
            last_tested_at: Utc::now().timestamp_millis(),
            state: TestConnectionState::Failure {
                message: response_body.clone(),
                request_payload: request_string,
            },
        },
    };

    let status_bson = bson::to_bson_with_options(&status, Default::default()).map_err(|e| {
        error!("Error serializing status to BSON: {:?}", e);

        InternalError::serialize_error("Could not serialize status to BSON", None)
    })?;

    state
        .app_stores
        .model_config
        .update_one(
            &connection_model_definition.id.to_string(),
            doc! {
                "$set": {
                    "testConnectionStatus": status_bson
                }
            },
        )
        .await
        .map_err(|e| {
            error!(
                "Error updating connection model definition in testing endpoint: {:?}",
                e
            );

            e
        })?;

    let response = TestConnectionResponse {
        code: status_code,
        status,
        response: response_body,
        meta: Meta {
            timestamp: Utc::now().timestamp_millis() as u64,
            platform: connection.platform.to_string(),
            platform_version: connection.platform_version.clone(),
            connection_definition_id: connection_model_definition.connection_definition_id,
            connection_key: connection.key.to_string(),
            model_name: connection_model_definition.model_name.clone(),
            action: connection_model_definition.action.clone(),
        },
    };

    Ok(Json(ServerResponse::new(
        "connection_model_definition",
        response,
    )))
}


#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Dummy)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    #[serde(rename = "_id")]
    pub id: Option<Id>,
    pub connection_platform: String,
    pub connection_definition_id: Id,
    pub platform_version: String,
    pub title: String,
    pub name: String,
    pub model_name: String,
    pub base_url: String,
    pub path: String,
    pub auth_method: AuthMethod,
    pub action_name: CrudAction,
    #[serde(with = "http_serde_ext_ios::method", rename = "action")]
    pub http_method: http::Method,
    #[serde(
        with = "http_serde_ext_ios::header_map::option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub headers: Option<HeaderMap>,
    pub query_params: Option<BTreeMap<String, String>>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub extractor_config: Option<ExtractorConfig>,
    pub schemas: SchemasInput,
    pub samples: SamplesInput,
    pub responses: Vec<ResponseBody>,
    pub version: Version, // the event-inc-version
    pub is_default_crud_mapping: Option<bool>,
    pub test_connection_payload: Option<Value>,
    pub test_connection_status: Option<TestConnection>,
    pub mapping: Option<CrudMapping>,
    pub paths: Option<ModelPaths>,
    pub supported: Option<bool>,
    pub active: Option<bool>,
    pub knowledge: Option<String>,
    pub tags: Option<Vec<String>>,
}

impl HookExt<ConnectionModelDefinition> for CreateRequest {}
impl PublicExt<ConnectionModelDefinition> for CreateRequest {}

impl RequestExt for CreateRequest {
    type Output = ConnectionModelDefinition;

    fn from(&self) -> Option<Self::Output> {
        let key = format!(
            "api::{}::{}::{}::{}::{}::{}",
            self.connection_platform,
            self.platform_version,
            self.model_name,
            self.action_name,
            self.path,
            self.name
        )
        .to_lowercase();

        let mut record = Self::Output {
            id: self
                .id
                .unwrap_or_else(|| Id::now(IdPrefix::ConnectionModelDefinition)),
            connection_platform: self.connection_platform.clone(),
            connection_definition_id: self.connection_definition_id,
            platform_version: self.platform_version.clone(),
            key,
            title: self.title.clone(),
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            platform_info: PlatformInfo::Api(ApiModelConfig {
                base_url: self.base_url.clone(),
                path: self.path.clone(),
                content: Default::default(),
                auth_method: self.auth_method.clone(),
                headers: self.headers.clone(),
                query_params: self.query_params.clone(),
                schemas: self.schemas.clone(),
                samples: self.samples.clone(),
                responses: self.responses.clone(),
                paths: self.paths.clone(),
            }),
            action: self.http_method.clone(),
            action_name: self.action_name.clone(),
            extractor_config: self.extractor_config.clone(),
            test_connection_status: self.test_connection_status.clone().unwrap_or_default(),
            test_connection_payload: self.test_connection_payload.clone(),
            is_default_crud_mapping: self.is_default_crud_mapping,
            mapping: self.mapping.clone(),
            record_metadata: Default::default(),
            supported: self.supported.unwrap_or(false),
            knowledge: self.knowledge.clone(),
        };
        record.record_metadata.version = self.version.clone();

        if let Some(tags) = &self.tags {
            record.record_metadata.tags.clone_from(tags);
        }

        Some(record)
    }

    fn update(&self, mut record: Self::Output) -> Self::Output {
        let key = format!(
            "api::{}::{}::{}::{}::{}::{}",
            self.connection_platform,
            self.platform_version,
            self.model_name,
            self.action_name,
            self.path,
            self.name
        )
        .to_lowercase();

        record.key = key;
        record
            .connection_platform
            .clone_from(&self.connection_platform);
        record.connection_definition_id = self.connection_definition_id;
        record.platform_version.clone_from(&self.platform_version);
        record.title.clone_from(&self.title);
        record.name.clone_from(&self.name);
        record.action = self.http_method.clone();
        record.action_name = self.action_name.clone();
        record.platform_info = PlatformInfo::Api(ApiModelConfig {
            base_url: self.base_url.clone(),
            path: self.path.clone(),
            content: Default::default(),
            auth_method: self.auth_method.clone(),
            headers: self.headers.clone(),
            query_params: self.query_params.clone(),
            schemas: self.schemas.clone(),
            samples: self.samples.clone(),
            responses: self.responses.clone(),
            paths: self.paths.clone(),
        });
        record.mapping.clone_from(&self.mapping);
        record.extractor_config.clone_from(&self.extractor_config);
        record.knowledge.clone_from(&self.knowledge);
        record.record_metadata.version.clone_from(&self.version);

        if let Some(tags) = &self.tags {
            record.record_metadata.tags.clone_from(tags);
        }

        if let Some(supported) = self.supported {
            record.supported = supported;
        }

        if let Some(active) = self.active {
            record.record_metadata.active = active;
        }

        if let Some(test_connection_payload) = &self.test_connection_payload {
            record.test_connection_payload = Some(test_connection_payload.clone());
        }

        if let Some(test_connection_status) = &self.test_connection_status {
            record.test_connection_status = test_connection_status.clone();
        }

        record
    }

    fn get_store(stores: AppStores) -> MongoStore<Self::Output> {
        stores.model_config.clone()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionItem {
    pub title: String,
    pub key: String,
    #[serde(with = "http_serde_ext_ios::method")]
    pub method: http::Method,
    pub platform: String,
}

pub async fn get_available_actions(
    headers: HeaderMap,
    Path(platform): Path<String>,
    query: Option<Query<BTreeMap<String, String>>>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServerResponse<ReadResponse<ActionItem>>>, PicaError> {
    let query = shape_mongo_filter(query, None, Some(headers));

    let mut filter = query.filter;
    filter.insert("connectionPlatform", platform.clone());
    filter.insert("supported", true);

    let store = state.app_stores.model_config.clone();

    let count_filter = filter.clone();
    let count = store.count(count_filter, None);

    let find = store.get_many(
        Some(filter),
        None,
        None,
        Some(query.limit),
        Some(query.skip),
    );

    let res = match try_join!(count, find) {
        Ok((total, rows)) => {
            let action_items = rows
                .into_iter()
                .map(|model_def| ActionItem {
                    title: model_def.title,
                    key: model_def.name,
                    method: model_def.action,
                    platform: model_def.connection_platform,
                })
                .collect();

            ReadResponse {
                rows: action_items,
                skip: query.skip,
                limit: query.limit,
                total,
            }
        }
        Err(e) => {
            error!("Error reading from store: {e}");
            return Err(e);
        }
    };

    Ok(Json(ServerResponse::new("Available Actions", res)))
}
