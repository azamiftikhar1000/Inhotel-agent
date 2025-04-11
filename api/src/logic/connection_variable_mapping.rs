use super::{delete, read, update, HookExt, PublicExt, RequestExt};
use crate::{
    router::ServerResponse,
    server::{AppState, AppStores},
};
use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::{patch, post},
    Extension, Router,
};
use bson::doc;
use chrono::Utc;
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
use std::sync::Arc;

pub fn get_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/",
            post(create_mapping)
                .get(read::<CreateRequest, ConnectionVariableMapping>),
        )
        .route(
            "/:id",
            patch(update::<CreateRequest, ConnectionVariableMapping>)
                .delete(delete::<CreateRequest, ConnectionVariableMapping>),
        )
}

async fn create_mapping(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Arc<EventAccess>>,
    Json(payload): Json<CreateRequest>,
) -> Result<impl IntoResponse, PicaError> {
    let stores = &state.app_stores;
    // Check if mapping already exists for this definition
    let mut filter = doc! {
        "connectionModelDefinitionId": payload.connection_model_definition_id.to_string(),
        "deleted": false,
    };
    filter.insert("ownership.buildableId", access.ownership.id.to_string());

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
