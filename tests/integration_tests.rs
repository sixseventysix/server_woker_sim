use server_worker_sim::*;
use std::collections::HashMap;
use std::time::Duration;
use std::thread;

#[test]
fn test_query_missing_key_in_task() {
    let s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "running".into());

    let update_map = HashMap::new();

    let task_id = s.create_task(query_map, update_map);
    s.query_task(task_id, "nonexistent_key");
}

#[test]
fn test_update_missing_id_in_task() {
    let s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("info".into(), "test".into());

    let update_map = HashMap::new();

    let task_id = s.create_task(query_map, update_map);
    s.update_task(task_id, "bad_update_id");
}

#[test]
fn test_query_nonexistent_task() {
    let s = ServerThread::new();
    s.query_task(999, "any_key");
}

#[test]
fn test_update_nonexistent_task() {
    let s = ServerThread::new();
    s.update_task(888, "some_update");
}

#[test]
fn test_task_throttling_behavior() {
    let s = ServerThread::new();
    for i in 0..6 {
        s.create_task(
            [("get_status".into(), "idle".into())].into(),
            [("mark_done".into(), Box::new(|| println!("Marked done")) as Box<dyn FnMut() + Send>)].into()
        );
    }
}

#[test]
fn test_queried_task_w_throttled_tasks() {
    let s = ServerThread::new();
    let mut task_id = [0; 6];
    for i in 0..6 {
        task_id[i] = s.create_task(
            [("get_status".into(), "idle".into())].into(),
            [("mark_done".into(), Box::new(|| println!("Marked done")) as Box<dyn FnMut() + Send>)].into()
        );
    }

    s.query_task(task_id[0], "get_status");
    s.update_task(task_id[1], "mark_done");
    s.query_task(task_id[2], "get_status");
    s.query_task(task_id[0], "invalid_query");
}