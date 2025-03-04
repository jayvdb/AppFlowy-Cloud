use crate::collab::util::generate_random_string;
use client_api::collab_sync::NUMBER_OF_UPDATE_TRIGGER_INIT_SYNC;
use client_api_test_util::*;
use collab_entity::CollabType;
use database_entity::dto::{AFAccessLevel, QueryCollabParams};
use serde_json::json;
use sqlx::types::uuid;
use std::time::Duration;
use tokio::time::sleep;
use tracing::trace;

#[tokio::test]
async fn sync_collab_content_after_reconnect_test() {
  let object_id = uuid::Uuid::new_v4().to_string();
  let collab_type = CollabType::Document;

  let mut test_client = TestClient::new_user().await;
  let workspace_id = test_client.workspace_id().await;
  test_client
    .open_collab(&workspace_id, &object_id, collab_type.clone())
    .await;

  // Disconnect the client and edit the collab. The updates will not be sent to the server.
  test_client.disconnect().await;
  for i in 0..=5 {
    test_client
      .collab_by_object_id
      .get_mut(&object_id)
      .unwrap()
      .collab
      .lock()
      .insert(&i.to_string(), i.to_string());
  }

  // it will return RecordNotFound error when trying to get the collab from the server
  let err = test_client
    .api_client
    .get_collab(QueryCollabParams::new(
      &object_id,
      collab_type.clone(),
      &workspace_id,
    ))
    .await
    .unwrap_err();
  assert!(err.is_record_not_found());

  // After reconnect the collab should be synced to the server.
  test_client.reconnect().await;
  // Wait for the messages to be sent
  test_client.wait_object_sync_complete(&object_id).await;

  assert_server_collab(
    &workspace_id,
    &mut test_client.api_client,
    &object_id,
    &collab_type,
    10,
    json!( {
      "0": "0",
      "1": "1",
      "2": "2",
      "3": "3",
      "4": "4",
      "5": "5",
    }),
  )
  .await;
}

#[tokio::test]
async fn same_client_with_diff_devices_edit_same_collab_test() {
  let collab_type = CollabType::Document;
  let registered_user = generate_unique_registered_user().await;
  let mut client_1 = TestClient::user_with_new_device(registered_user.clone()).await;
  let mut client_2 = TestClient::user_with_new_device(registered_user.clone()).await;

  let workspace_id = client_1.workspace_id().await;
  let object_id = client_1
    .create_and_edit_collab(&workspace_id, collab_type.clone())
    .await;

  // client 1 edit the collab
  client_1
    .collab_by_object_id
    .get_mut(&object_id)
    .unwrap()
    .collab
    .lock()
    .insert("name", "work");
  client_1.wait_object_sync_complete(&object_id).await;

  client_2
    .open_collab(&workspace_id, &object_id, collab_type.clone())
    .await;
  sleep(Duration::from_millis(1000)).await;

  trace!("client 2 disconnect: {:?}", client_2.device_id);
  client_2.disconnect().await;
  sleep(Duration::from_millis(1000)).await;

  client_2
    .collab_by_object_id
    .get_mut(&object_id)
    .unwrap()
    .collab
    .lock()
    .insert("name", "workspace");

  client_2.reconnect().await;
  client_2.wait_object_sync_complete(&object_id).await;

  let expected_json = json!({
    "name": "workspace"
  });
  assert_client_collab_within_30_secs(&mut client_1, &object_id, "name", expected_json.clone())
    .await;
  assert_client_collab_within_30_secs(&mut client_2, &object_id, "name", expected_json.clone())
    .await;
}

#[tokio::test]
async fn same_client_with_diff_devices_edit_diff_collab_test() {
  let registered_user = generate_unique_registered_user().await;
  let collab_type = CollabType::Document;
  let mut device_1 = TestClient::user_with_new_device(registered_user.clone()).await;
  let mut device_2 = TestClient::user_with_new_device(registered_user.clone()).await;

  let workspace_id = device_1.workspace_id().await;

  // different devices create different collabs. the collab will be synced between devices
  let object_id_1 = device_1
    .create_and_edit_collab(&workspace_id, collab_type.clone())
    .await;
  let object_id_2 = device_2
    .create_and_edit_collab(&workspace_id, collab_type.clone())
    .await;

  // client 1 edit the collab with object_id_1
  device_1
    .collab_by_object_id
    .get_mut(&object_id_1)
    .unwrap()
    .collab
    .lock()
    .insert("name", "object 1");
  device_1.wait_object_sync_complete(&object_id_1).await;

  // client 2 edit the collab with object_id_2
  device_2
    .collab_by_object_id
    .get_mut(&object_id_2)
    .unwrap()
    .collab
    .lock()
    .insert("name", "object 2");
  device_2.wait_object_sync_complete(&object_id_2).await;

  // client1 open the collab with object_id_2
  device_1
    .open_collab(&workspace_id, &object_id_2, collab_type.clone())
    .await;
  assert_client_collab_within_30_secs(
    &mut device_1,
    &object_id_2,
    "name",
    json!({
      "name": "object 2"
    }),
  )
  .await;

  // client2 open the collab with object_id_1
  device_2
    .open_collab(&workspace_id, &object_id_1, collab_type.clone())
    .await;
  assert_client_collab_within_30_secs(
    &mut device_2,
    &object_id_1,
    "name",
    json!({
      "name": "object 1"
    }),
  )
  .await;
}

#[tokio::test]
async fn edit_document_with_both_clients_offline_then_online_sync_test() {
  let collab_type = CollabType::Document;
  let mut client_1 = TestClient::new_user().await;
  let mut client_2 = TestClient::new_user().await;

  let workspace_id = client_1.workspace_id().await;
  let object_id = client_1
    .create_and_edit_collab(&workspace_id, collab_type.clone())
    .await;

  // add client 2 as a member of the collab
  client_1
    .add_client_as_collab_member(
      &workspace_id,
      &object_id,
      &client_2,
      AFAccessLevel::ReadAndWrite,
    )
    .await;
  client_1.disconnect().await;

  client_2
    .open_collab(&workspace_id, &object_id, collab_type.clone())
    .await;
  client_2.disconnect().await;

  for i in 0..10 {
    if i % 2 == 0 {
      client_1
        .collab_by_object_id
        .get_mut(&object_id)
        .unwrap()
        .collab
        .lock()
        .insert(&i.to_string(), format!("Task {}", i));
    } else {
      client_2
        .collab_by_object_id
        .get_mut(&object_id)
        .unwrap()
        .collab
        .lock()
        .insert(&i.to_string(), format!("Task {}", i));
    }
  }

  tokio::join!(client_1.reconnect(), client_2.reconnect());
  tokio::join!(
    client_1.wait_object_sync_complete(&object_id),
    client_2.wait_object_sync_complete(&object_id)
  );

  let expected_json = json!({
    "0": "Task 0",
    "1": "Task 1",
    "2": "Task 2",
    "3": "Task 3",
    "4": "Task 4",
    "5": "Task 5",
    "6": "Task 6",
    "7": "Task 7",
    "8": "Task 8",
    "9": "Task 9"
  });
  assert_client_collab_include_value_within_30_secs(
    &mut client_1,
    &object_id,
    expected_json.clone(),
  )
  .await;
  assert_client_collab_include_value_within_30_secs(
    &mut client_2,
    &object_id,
    expected_json.clone(),
  )
  .await;
}

#[tokio::test]
async fn init_sync_when_missing_updates_test() {
  let text = generate_random_string(1024);
  let collab_type = CollabType::Document;
  let mut client_1 = TestClient::new_user().await;
  let mut client_2 = TestClient::new_user().await;

  // Create a collaborative document with client_1 and invite client_2 to collaborate.
  let workspace_id = client_1.workspace_id().await;
  let object_id = client_1
    .create_and_edit_collab(&workspace_id, collab_type.clone())
    .await;
  client_1
    .add_client_as_collab_member(
      &workspace_id,
      &object_id,
      &client_2,
      AFAccessLevel::ReadAndWrite,
    )
    .await;

  // Client_1 makes the first edit by inserting "task 1".
  client_1
    .collab_by_object_id
    .get_mut(&object_id)
    .unwrap()
    .collab
    .lock()
    .insert("1", "task 1");
  client_1.wait_object_sync_complete(&object_id).await;

  // Client_2 opens the collaboration, triggering an initial sync to receive "task 1".
  client_2
    .open_collab(&workspace_id, &object_id, collab_type.clone())
    .await;
  client_2.wait_object_sync_complete(&object_id).await;

  // Validate both clients have "task 1" after the initial sync.
  assert_eq!(
    client_1.get_edit_collab_json(&object_id).await,
    json!({ "1": "task 1" })
  );
  assert_eq!(
    client_2.get_edit_collab_json(&object_id).await,
    json!({ "1": "task 1" })
  );

  // Simulate client_2 missing updates by enabling skip_realtime_message.
  client_2.ws_client.disable_receive_message();
  client_1.wait_object_sync_complete(&object_id).await;

  // Client_1 inserts "task 2", which client_2 misses due to skipping realtime messages.
  for _ in 0..NUMBER_OF_UPDATE_TRIGGER_INIT_SYNC {
    client_1
      .collab_by_object_id
      .get_mut(&object_id)
      .unwrap()
      .collab
      .lock()
      .insert("2", text.clone());
    sleep(Duration::from_millis(300)).await;
  }
  client_1.wait_object_sync_complete(&object_id).await;

  client_2
    .collab_by_object_id
    .get_mut(&object_id)
    .unwrap()
    .collab
    .lock()
    .insert("3", "task 3");
  client_2.wait_object_sync_complete(&object_id).await;

  // Validate client_1's view includes "task 2", and "task 3", while client_2 missed key2 and key3.
  assert_client_collab_include_value_within_30_secs(
    &mut client_1,
    &object_id,
    json!({ "1": "task 1", "2": text.clone(), "3": "task 3" }),
  )
  .await;
  assert_eq!(
    client_2.get_edit_collab_json(&object_id).await,
    json!({ "1": "task 1", "3": "task 3" })
  );

  // client_2 resumes receiving messages
  // client_1 triggers a sync that will trigger broadcast message to client_2
  client_2.ws_client.enable_receive_message();
  client_1
    .collab_by_object_id
    .get_mut(&object_id)
    .unwrap()
    .collab
    .lock()
    .insert("4", "task 4");
  client_1.wait_object_sync_complete(&object_id).await;

  assert_client_collab_include_value_within_30_secs(
    &mut client_2,
    &object_id,
    json!({ "1": "task 1", "2": text.clone(), "3": "task 3", "4": "task 4" }),
  )
  .await;
}
