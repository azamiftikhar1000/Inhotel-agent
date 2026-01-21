use crate::{
    id::Id,
    prelude::shared::{
        ownership::Ownership,
        record_metadata::RecordMetadata,
    },
    configuration::environment::Environment,
};
use serde::{Deserialize, Serialize};

/// Mapping between connection variables and model definition parameters.
/// Defines how per-connection variables are substituted into API calls.
/// Scoped at the Platform/Definition level (applies to ALL connections using this definition).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "dummy", derive(fake::Dummy))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionVariableMapping {
    #[serde(rename = "_id")]
    pub id: Id,
    
    /// The model definition this mapping applies to (Platform Level)
    pub connection_model_definition_id: Id,
    
    /// The platform this mapping belongs to (e.g., "blaze", "salesforce")
    /// Used for filtering and grouping mappings
    pub connection_platform: String,
    
    /// List of variable-to-parameter bindings
    pub bindings: Vec<VariableBinding>,
    
    /// Ownership information for multi-tenancy
    pub ownership: Ownership,
    
    /// Environment (test/live)
    pub environment: Environment,
    
    #[serde(flatten, default)]
    pub record_metadata: RecordMetadata,
}

/// A single binding that maps a connection variable to a target parameter
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "dummy", derive(fake::Dummy))]
#[serde(rename_all = "camelCase")]
pub struct VariableBinding {
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

/// Where to inject the variable value in the API request
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[cfg_attr(feature = "dummy", derive(fake::Dummy))]
pub enum ParameterLocation {
    /// URL path parameter: /hotels/{id}
    PathParam,
    /// Query string parameter: ?hotel_id=123
    QueryParam,
    /// HTTP header: X-Hotel-Id: 123
    Header,
    /// JSON body field: {"hotelId": "123"}
    BodyField,
}

/// Strategy for injecting the variable
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[cfg_attr(feature = "dummy", derive(fake::Dummy))]
pub enum InjectionStrategy {
    /// Always overwrite user input (Default, Secure)
    Strict,
    /// Only inject if parameter is missing (Flexible)
    Fallback,
    /// Append to existing value (for Lists)
    Append,
}

impl Default for InjectionStrategy {
    fn default() -> Self {
        Self::Strict
    }
}

/// Expected data type of the variable
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[cfg_attr(feature = "dummy", derive(fake::Dummy))]
pub enum VariableDataType {
    String,
    Number,
    Boolean,
    /// Parse as JSON (Object or Array)
    Json,
}

impl Default for VariableDataType {
    fn default() -> Self {
        Self::String
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_serialize_connection_variable_mapping() {
        let binding = VariableBinding {
            variable_name: "hotel_id".to_string(),
            target_param: "id".to_string(),
            location: ParameterLocation::PathParam,
        };

        let json_val = serde_json::to_value(&binding).unwrap();
        assert_eq!(json_val["variableName"], "hotel_id");
        assert_eq!(json_val["targetParam"], "id");
        assert_eq!(json_val["location"], "PathParam");
    }

    #[test]
    fn test_deserialize_parameter_location() {
        let path_param: ParameterLocation = serde_json::from_value(json!("PathParam")).unwrap();
        assert_eq!(path_param, ParameterLocation::PathParam);

        let query_param: ParameterLocation = serde_json::from_value(json!("QueryParam")).unwrap();
        assert_eq!(query_param, ParameterLocation::QueryParam);

        let header: ParameterLocation = serde_json::from_value(json!("Header")).unwrap();
        assert_eq!(header, ParameterLocation::Header);

        let body_field: ParameterLocation = serde_json::from_value(json!("BodyField")).unwrap();
        assert_eq!(body_field, ParameterLocation::BodyField);
    }
}
