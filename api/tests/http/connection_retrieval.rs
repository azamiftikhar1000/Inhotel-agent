use crate::context::TestServer;
use api::logic::{connection_definition::CreateRequest, ReadResponse};
use fake::{Fake, Faker};
use http::Method;
use osentities::{
    connection_definition::ConnectionDefinition, environment::Environment, SanitizedConnection,
};
use serde_json::Value;

#[tokio::test]
async fn test_get_connections_with_definition_name() {
    // 100 items cache size
    let mut server = TestServer::new_with_cache(None, Some("100".to_string())).await;
    
    // Create a connection (this helper creates both definition and connection)
    let (connection, _model_def) = server.create_connection(Environment::Live).await;
    
    // Fetch connections
    let res = server
        .send_request::<Value, Value>(
            "v1/connections",
            Method::GET,
            Some(&server.live_key),
            None,
        )
        .await
        .unwrap();

    assert!(res.code.is_success());
    
    let res = serde_json::from_value::<ReadResponse<SanitizedConnection>>(res.data).unwrap();
    
    // Assert we found our connection
    let fetched_conn = res.rows.iter().find(|c| c.id == connection.id).unwrap();
    
    // Assert name is populated
    assert!(fetched_conn.connection_definition_name.is_some());
    
    // Validate it matches the definition name
    let def_res = server
        .send_request::<Value, Value>(
            &format!("v1/public/connection-definitions?_id={}", connection.connection_definition_id),
            Method::GET,
            Some(&server.live_key),
            None,
        )
        .await
        .unwrap();
        
     let def_res = serde_json::from_value::<ReadResponse<ConnectionDefinition>>(def_res.data).unwrap();
     let def_name = def_res.rows[0].name.clone();
     
     assert_eq!(fetched_conn.connection_definition_name.as_ref().unwrap(), &def_name);
}

#[tokio::test]
async fn test_update_connection_definition_cache_invalidation() {
    let mut server = TestServer::new_with_cache(None, Some("100".to_string())).await;
    let (connection, _model_def) = server.create_connection(Environment::Live).await;

    // 1. Fetch initially to populate cache
    let res = server
        .send_request::<Value, Value>(
            "v1/connections",
            Method::GET,
            Some(&server.live_key),
            None,
        )
        .await
        .unwrap();
    let res = serde_json::from_value::<ReadResponse<SanitizedConnection>>(res.data).unwrap();
    let fetched_conn = res.rows.iter().find(|c| c.id == connection.id).unwrap();
    let original_name = fetched_conn.connection_definition_name.clone().unwrap();

    // 2. Update definition name
    let new_name = "Updated Definition Name".to_string();
    
    // Generate valid payload using Faker, but override name
    let mut payload: CreateRequest = Faker.fake();
    payload.name = new_name.clone();
    // We must ensure the platform/version matches the existing one to avoid key regeneration logic that might conflict or just to be safe
    // But Faker generates random strings. 
    // Let's fetch the original definition to get platform/version
    let def_res = server
        .send_request::<Value, Value>(
            &format!("v1/public/connection-definitions?_id={}", connection.connection_definition_id),
            Method::GET,
            Some(&server.live_key),
            None,
        )
        .await
        .unwrap();
    let def_res = serde_json::from_value::<ReadResponse<ConnectionDefinition>>(def_res.data).unwrap();
    let original_def = def_res.rows[0].clone();
    
    payload.platform = original_def.platform;
    payload.platform_version = original_def.platform_version;
    payload.id = Some(original_def.id);

    // Perform update
    let update_res = server
        .send_request::<CreateRequest, Value>(
            &format!("v1/connection-definitions/{}", connection.connection_definition_id),
            Method::PATCH,
            Some(&server.live_key),
            Some(&payload),
        )
        .await
        .unwrap();
        
    assert!(update_res.code.is_success());
    
    // 3. Fetch connections again - should have new name if cache was invalidated
    // (And if using cache size > 0, which we configured)
    let res = server
        .send_request::<Value, Value>(
            "v1/connections",
            Method::GET,
            Some(&server.live_key),
            None,
        )
        .await
        .unwrap();
        
    let res = serde_json::from_value::<ReadResponse<SanitizedConnection>>(res.data).unwrap();
    let fetched_conn = res.rows.iter().find(|c| c.id == connection.id).unwrap();
    
    assert_eq!(fetched_conn.connection_definition_name.as_ref().unwrap(), &new_name);
    assert_ne!(fetched_conn.connection_definition_name.as_ref().unwrap(), &original_name);
}
