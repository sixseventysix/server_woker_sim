use server_worker_sim::*;
use std::collections::HashMap;
use std::time::Duration;
use std::thread;

#[test]
fn test_simple_success() {
    let s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "running".into());

    let update_map = HashMap::new();

    let task_id = s.create_task(query_map, update_map);
    s.query_task(task_id, "status");
    s.shutdown();
}

#[test]
fn test_query_after_completion() {
    let s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "running".into());
    let update_map = HashMap::new();
    let task_id = s.create_task(query_map, update_map);
    thread::sleep(Duration::from_secs(TASK_TIMEOUT + 1)); // make sure time is more than TASK_TIMEOUT and less than LISTENER_TIMEOUT
    s.query_task(task_id, "status");
    s.shutdown();
}

#[test]
fn test_query_after_worker_dropped() {
    let s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "running".into());
    let update_map = HashMap::new();
    let task_id = s.create_task(query_map, update_map);
    thread::sleep(Duration::from_secs(LISTENER_TIMEOUT + 1));
    s.query_task(task_id, "status");
    s.shutdown();
}

#[test]
fn test_query_missing_key_in_task() {
    let s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "running".into());

    let update_map = HashMap::new();

    let task_id = s.create_task(query_map, update_map);
    s.query_task(task_id, "nonexistent_key");
    s.shutdown();
}

#[test]
fn test_update_missing_id_in_task() {
    let s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("info".into(), "test".into());

    let update_map = HashMap::new();

    let task_id = s.create_task(query_map, update_map);
    s.update_task(task_id, "bad_update_id");
    s.shutdown();
}

#[test]
fn test_query_nonexistent_task() {
    let s = ServerThread::new();
    s.query_task(999, "any_key");
    s.shutdown();
}

#[test]
fn test_update_nonexistent_task() {
    let s = ServerThread::new();
    s.update_task(888, "some_update");
    s.shutdown();
}

#[test]
fn test_task_throttling_behavior() {
    let s = ServerThread::new();
    for _i in 0..6 {
        s.create_task(
            [("get_status".into(), "idle".into())].into(),
            [("mark_done".into(), Box::new(|| "Done".to_string()) as Box<dyn FnMut() -> String + Send>)].into()
        );
    }
    s.shutdown();
}

#[test]
fn test_queried_task_w_throttled_tasks() {
    let s = ServerThread::new();
    let mut task_id = [0; 6];
    for i in 0..6 {
        let update_map = HashMap::from([
            (
                "mark_done".into(),
                Box::new(|| "done".to_string()) as Box<dyn FnMut() -> String + Send + 'static>,
            ),
        ]);
        task_id[i] = s.create_task(            
            [("get_status".into(), "idle".into())].into(),
            update_map
        );
    }

    s.query_task(task_id[0], "get_status");
    s.update_task(task_id[1], "mark_done");
    s.query_task(task_id[2], "get_status");
    s.query_task(task_id[0], "invalid_query");
    s.shutdown();
}

#[test]
fn test_complex_task_interactions() {
    let server = ServerThread::new();

    let mut task_ids = Vec::new();
    for i in 0..4 {
        let update_map = HashMap::from([
            (
                "mark_done".into(),
                Box::new(|| "done".to_string()) as Box<dyn FnMut() -> String + Send + 'static>,
            ),
        ]);
        let task_id = server.create_task(
            [("get_status".into(), format!("idle_{}", i))].into(),
            update_map           
        );
        task_ids.push(task_id);
    }

    server.query_task(task_ids[0], "get_status");
    server.update_task(task_ids[1], "mark_done");
    server.query_task(task_ids[2], "get_status");
    server.query_task(task_ids[0], "invalid_query");
    server.update_task(task_ids[3], "invalid_update");
    server.query_task(task_ids[0], "get_status");
    server.update_task(task_ids[1], "mark_done");
    server.query_task(task_ids[2], "get_status");
    server.update_task(task_ids[3], "mark_done");
    server.query_task(task_ids[1], "get_status");

    server.shutdown();
}

#[test]
fn test_multiple_queries_in_quick_succession() {
    let s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "busy".into());

    let task_id = s.create_task(query_map, HashMap::new());

    for _ in 0..10 {
        s.query_task(task_id, "status");
    }
    s.shutdown();
}

#[test]
fn test_throttling_recovery_after_timeout() {
    let s = ServerThread::new();
    let mut task_ids = vec![];

    for _ in 0..MAX_CONCURRENT_TASKS {
        let id = s.create_task(
            [("info".into(), "live".into())].into(),
            HashMap::new(),
        );
        task_ids.push(id);
    }

    // these should be throttled
    for _ in 0..2 {
        s.create_task(
            [("info".into(), "extra".into())].into(),
            HashMap::new(),
        );
    }

    thread::sleep(Duration::from_secs(TASK_TIMEOUT + 1));  // wait for some tasks to timeout

    let id = s.create_task(
        [("info".into(), "retry".into())].into(),
        HashMap::new(),
    );

    s.query_task(id, "info");

    s.shutdown();
}

