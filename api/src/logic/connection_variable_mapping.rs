use super::{HookExt, PublicExt, ReadResponse, RequestExt, SuccessResponse};
use crate::{
    helper::shape_mongo_filter,
    router::ServerResponse,
    server::{AppState, AppStores},
};
use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Extension, Router,
};
use bson::doc;
use chrono::Utc;
use http::HeaderMap;
use osentities::{
    algebra::MongoStore,
    connection_variable_mapping::{
        ConnectionVariableMapping, InjectionStrategy, ParameterLocation, VariableBinding,
        VariableDataType,
    },
    event_access::EventAccess,
    id::{prefix::IdPrefix, Id},
    record_metadata::RecordMetadata,
    ApplicationError, InternalError, PicaError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tracing::error;

pub fn get_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/",
            post(create_mapping)
                .get(read_mappings), // Custom handler without ownership filtering
        )
        .route(
            "/:id",
            patch(update_mapping)  // Custom handler without ownership filtering
                .delete(delete_mapping), // Custom handler without ownership filtering
        )
}

/// Custom read handler that returns ALL platform-level mappings without ownership filtering.
/// This is necessary because ConnectionVariableMappings are shared across all users of a platform.
async fn read_mappings(
    headers: HeaderMap,
    query: Option<Query<BTreeMap<String, String>>>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServerResponse<ReadResponse<Value>>>, PicaError> {
    // Pass None for event_access to bypass ownership filtering
    let query_params = shape_mongo_filter(query, None, Some(headers));

    let store = state.app_stores.connection_variable_mapping.clone();

    let rows: Vec<ConnectionVariableMapping> = store
        .get_many(
            Some(query_params.filter.clone()),
            None,
            None,
            Some(query_params.limit),
            Some(query_params.skip),
        )
        .await?;

    let total = store.count(query_params.filter, None).await?;

    let res = ReadResponse {
        rows: rows.into_iter().map(CreateRequest::public).collect(),
        skip: query_params.skip,
        limit: query_params.limit,
        total,
    };

    Ok(Json(ServerResponse::new("read", res)))
}

/// Custom update handler without ownership filtering.
/// Platform-level mappings can be updated by any authenticated user.
async fn update_mapping(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateRequest>,
) -> Result<Json<ServerResponse<SuccessResponse>>, PicaError> {
    let store = state.app_stores.connection_variable_mapping.clone();

    // Platform-level: no ownership filter, just find by ID
    let filter = doc! {
        "_id": &id,
        "deleted": false,
    };

    let Some(record) = store.get_one(filter).await? else {
        return Err(ApplicationError::not_found(
            &format!("Mapping with id {} not found", id),
            None,
        ));
    };

    let updated_record = payload.update(record);

    let bson = bson::to_bson_with_options(&updated_record, Default::default()).map_err(|e| {
        error!("Could not serialize record into document: {e}");
        InternalError::serialize_error(e.to_string().as_str(), None)
    })?;

    let document = doc! { "$set": bson };

    store.update_one(&id, document).await?;

    Ok(Json(ServerResponse::new(
        "update",
        SuccessResponse { success: true },
    )))
}

/// Custom delete handler without ownership filtering.
/// Platform-level mappings can be deleted by any authenticated user (soft delete).
async fn delete_mapping(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServerResponse<Value>>, PicaError> {
    let store = state.app_stores.connection_variable_mapping.clone();

    // Platform-level: no ownership filter, just find by ID
    let filter = doc! {
        "_id": &id,
        "deleted": false,
    };

    let Some(record) = store.get_one(filter).await? else {
        return Err(ApplicationError::not_found(
            &format!("Mapping with id {} not found", id),
            None,
        ));
    };

    // Soft delete
    store
        .update_one(
            &id,
            doc! {
                "$set": {
                    "deleted": true,
                }
            },
        )
        .await?;

    Ok(Json(ServerResponse::new("delete", CreateRequest::public(record))))
}

async fn create_mapping(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Arc<EventAccess>>,
    Json(payload): Json<CreateRequest>,
) -> Result<impl IntoResponse, PicaError> {
    let stores = &state.app_stores;
    // Check if mapping already exists for this definition (platform-level, no ownership filter)
    // Mappings are shared across all users of a platform, so uniqueness is global
    let filter = doc! {
        "connectionModelDefinitionId": payload.connection_model_definition_id.to_string(),
        "deleted": false,
    };

    let existing = stores
        .connection_variable_mapping
        .get_many(
            Some(filter),
            None,
            None,
            Some(1), // Limit 1
            None,
        )
        .await
        .map_err(PicaError::from)?;

    if !existing.is_empty() {
        return Err(ApplicationError::conflict(
            &format!(
                "Mapping already exists for model definition {}",
                payload.connection_model_definition_id
            ),
            None,
        ).into());
    }

    // Proceed with creation using standard logic
    let record = payload.access(access.clone()).ok_or_else(|| {
        InternalError::unknown("Failed to create record from request", None)
    })?;

    let created = stores
        .connection_variable_mapping
        .create_one(&record)
        .await
        .map_err(PicaError::from)?;
    
    Ok((StatusCode::CREATED, Json(ServerResponse::new("create", CreateRequest::public(record)))))
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    #[serde(rename = "_id")]
    pub id: Option<Id>,

    /// The model definition this mapping applies to (Platform Level)
    pub connection_model_definition_id: Id,

    /// The platform this mapping belongs to (e.g., "blaze", "salesforce")
    pub connection_platform: String,

    /// List of variable-to-parameter bindings
    pub bindings: Vec<BindingRequest>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingRequest {
    /// Name of the variable in secrets (e.g., "hotel_id", "SALESFORCE_DOMAIN")
    pub variable_name: String,

    /// Target parameter name in the API call (e.g., "id", "domain")
    pub target_param: String,

    /// Where to inject the value
    pub location: ParameterLocation,

    /// How to inject the value
    #[serde(default)]
    pub strategy: InjectionStrategy,

    /// Data type of the variable (for conversion)
    #[serde(default)]
    pub data_type: VariableDataType,
}

impl HookExt<ConnectionVariableMapping> for CreateRequest {}
impl PublicExt<ConnectionVariableMapping> for CreateRequest {}

impl RequestExt for CreateRequest {
    type Output = ConnectionVariableMapping;

    fn access(&self, event_access: Arc<EventAccess>) -> Option<Self::Output> {
        Some(Self::Output {
            id: self
                .id
                .unwrap_or_else(|| Id::now(IdPrefix::ConnectionVariableMapping)),
            connection_model_definition_id: self.connection_model_definition_id,
            connection_platform: self.connection_platform.clone(),
            bindings: self
                .bindings
                .iter()
                .map(|b| VariableBinding {
                    variable_name: b.variable_name.clone(),
                    target_param: b.target_param.clone(),
                    location: b.location.clone(),
                    strategy: b.strategy.clone(),
                    data_type: b.data_type.clone(),
                })
                .collect(),
            ownership: event_access.ownership.clone(),
            environment: event_access.environment.clone(),
            record_metadata: RecordMetadata::default(),
        })
    }

    // from() returns None because this endpoint requires authentication
    // and we need ownership from EventAccess

    fn update(&self, mut record: Self::Output) -> Self::Output {
        record.connection_model_definition_id = self.connection_model_definition_id;
        record.connection_platform = self.connection_platform.clone();
        record.bindings = self
            .bindings
            .iter()
            .map(|b| VariableBinding {
                variable_name: b.variable_name.clone(),
                target_param: b.target_param.clone(),
                location: b.location.clone(),
                strategy: b.strategy.clone(),
                data_type: b.data_type.clone(),
            })
            .collect();
        record.record_metadata.updated_at = Utc::now().timestamp_millis();
        record.record_metadata.updated = true;

        record
    }

    fn get_store(stores: AppStores) -> MongoStore<Self::Output> {
        stores.connection_variable_mapping.clone()
    }
}
