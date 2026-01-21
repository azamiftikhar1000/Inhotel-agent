use super::{ReadResponse, HookExt, PublicExt, RequestExt};
use crate::{
    helper::shape_mongo_filter,
    router::ServerResponse,
    server::{AppState, AppStores},
};
use axum::{
    extract::{Query, State},
    routing::get,
    Router, Json,
};
use bson::doc;
use fake::Dummy;
use http::HeaderMap;
use osentities::{
    connection_variable_mapping::{ConnectionVariableMapping, InjectionStrategy},
    record_metadata::RecordMetadata,
    Id, MongoStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tracing::error;

pub fn get_router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(read_knowledge))
}

/// Custom read handler that enriches knowledge with mapping annotations
async fn read_knowledge(
    headers: HeaderMap,
    query: Option<Query<BTreeMap<String, String>>>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServerResponse<ReadResponse<Value>>>, osentities::PicaError> {
    let query_params = shape_mongo_filter(query, None, Some(headers));

    let store = state.app_stores.knowledge.clone();
    let mapping_store = state.app_stores.connection_variable_mapping.clone();

    // Fetch knowledge records
    let rows: Vec<Knowledge> = store
        .get_many(
            Some(query_params.filter.clone()),
            None,
            None,
            Some(query_params.limit),
            Some(query_params.skip),
        )
        .await?;

    let total = store.count(query_params.filter, None).await?;

    // Batch fetch ALL mappings for these definitions in a single query
    let definition_ids: Vec<String> = rows.iter().map(|r| r.id.to_string()).collect();
    let all_mappings: Vec<ConnectionVariableMapping> = if !definition_ids.is_empty() {
        mapping_store
            .get_many(
                Some(doc! {
                    "connectionModelDefinitionId": { "$in": &definition_ids },
                    "deleted": false,
                }),
                None,
                None,
                None, // No limit - get all
                None,
            )
            .await
            .unwrap_or_else(|e| {
                error!("Error batch fetching mappings: {:?}", e);
                Vec::new()
            })
    } else {
        Vec::new()
    };

    // Build HashMap for O(1) lookup
    let mapping_map: std::collections::HashMap<String, ConnectionVariableMapping> = all_mappings
        .into_iter()
        .map(|m| (m.connection_model_definition_id.to_string(), m))
        .collect();

    // Enrich each record with mapping annotations
    let mut enriched_rows: Vec<Value> = Vec::with_capacity(rows.len());
    for mut record in rows {
        // O(1) lookup
        if let Some(m) = mapping_map.get(&record.id.to_string()) {
            let mut annotations = String::from("IMPORTANT: ");
            let mut param_list: Vec<String> = Vec::new();
            for binding in &m.bindings {
                match binding.strategy {
                    InjectionStrategy::Strict => {
                        param_list.push(format!("'{}' (auto-filled, do NOT ask user)", binding.target_param));
                    }
                    InjectionStrategy::Fallback => {
                        param_list.push(format!("'{}' (has default, only ask if user wants to override)", binding.target_param));
                    }
                    InjectionStrategy::Append => {
                        param_list.push(format!("'{}' (partially pre-filled, user may add more)", binding.target_param));
                    }
                }
            }
            annotations.push_str(&format!(
                "The following parameters are automatically handled by the system and do NOT need to be retrieved or asked for: {}.\n\n",
                param_list.join(", ")
            ));
            // Prepend to existing knowledge
            record.knowledge = Some(
                record
                    .knowledge
                    .map(|k| format!("{}{}", annotations, k))
                    .unwrap_or(annotations),
            );
        }

        enriched_rows.push(serde_json::to_value(&record).unwrap_or_default());
    }

    Ok(Json(ServerResponse::new(
        "read",
        ReadResponse {
            rows: enriched_rows,
            skip: query_params.skip,
            limit: query_params.limit,
            total,
        },
    )))
}

struct ReadRequest;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Dummy)]
#[serde(rename_all = "camelCase")]
pub struct Knowledge {
    #[serde(rename = "_id")]
    pub id: Id,
    pub connection_platform: String,
    pub title: String,
    pub path: String,
    pub knowledge: Option<String>,
    pub base_url: String,
    #[serde(flatten)]
    pub metadata: RecordMetadata,
}

impl HookExt<Knowledge> for ReadRequest {}
impl PublicExt<Knowledge> for ReadRequest {}
impl RequestExt for ReadRequest {
    type Output = Knowledge;

    fn get_store(stores: AppStores) -> MongoStore<Self::Output> {
        stores.knowledge
    }
}
