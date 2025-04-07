use server_worker_sim::*;
use std::collections::HashMap;
use std::time::Duration;
use std::thread;

#[test]
fn test_simple_success() {
    let mut s = ServerThread::new();

    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "running".into());
    let task_id = s.create_task(query_map, HashMap::new());
    s.query_task(task_id, "status");
    s.join_listener();

    assert!(s.expect(1, &TaskResult::QueryOk {
        req_id: 1,
        id: task_id,
        value: "running".into()
    }));
}

#[test]
fn test_query_after_completion() {
    let mut s = ServerThread::new();
    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "running".into());
    let task_id = s.create_task(query_map, HashMap::new());
    std::thread::sleep(Duration::from_secs(TASK_TIMEOUT + 1));
    s.query_task(task_id, "status");
    s.join_listener();

    assert!(s.expect(1, &TaskResult::NotFound {
        req_id: 1,
        id: task_id,
        ctx: "Task not found for query"
    }));
}

#[test]
fn test_query_after_worker_dropped() {
    let mut s = ServerThread::new();
    let mut query_map = HashMap::new();
    query_map.insert("status".into(), "running".into());
    let task_id = s.create_task(query_map, HashMap::new());
    std::thread::sleep(Duration::from_secs(LISTENER_TIMEOUT + 1));
    s.query_task(task_id, "status");
    s.join_listener();

    assert!(s.expect_none(1));
}

#[test]
fn test_query_missing_key_in_task() {
    let mut s = ServerThread::new();
    let task_id = s.create_task([("status".into(), "running".into())].into(), HashMap::new());
    s.query_task(task_id, "nonexistent_key");
    s.join_listener();

    assert!(s.expect(1, &TaskResult::QueryError {
        req_id: 1,
        id: task_id,
        msg: "Query ID 'nonexistent_key' not found".into()
    }));
}

#[test]
fn test_update_missing_id_in_task() {
    let mut s = ServerThread::new();
    let task_id = s.create_task([("info".into(), "test".into())].into(), HashMap::new());
    s.update_task(task_id, "bad_update_id");
    s.join_listener();

    assert!(s.expect(1, &TaskResult::UpdateError {
        req_id: 1,
        id: task_id,
        msg: "Update ID 'bad_update_id' not found".into()
    }));
}

#[test]
fn test_query_nonexistent_task() {
    let mut s = ServerThread::new();
    s.query_task(999, "any_key");
    s.join_listener();

    assert!(s.expect(0, &TaskResult::NotFound {
        req_id: 0,
        id: 999,
        ctx: "Task not found for query"
    }));
}

#[test]
fn test_update_nonexistent_task() {
    let mut s = ServerThread::new();
    s.update_task(888, "some_update");
    s.join_listener();

    assert!(s.expect(0, &TaskResult::NotFound {
        req_id: 0,
        id: 888,
        ctx: "Task not found for update"
    }));
}

#[test]
fn test_task_throttling_behavior() {
    let mut s = ServerThread::new();
    let mut throttled_ids = vec![];

    for i in 0..6 {
        let id = s.create_task(
            [("get_status".into(), "idle".into())].into(),
            [("mark_done".into(), Box::new(|| "Done".to_string()) as Box<dyn FnMut() -> String + Send>)].into()
        );
        if i >= MAX_CONCURRENT_TASKS {
            throttled_ids.push((i , id));
        }
    }

    s.join_listener();
    for (req_id, id) in throttled_ids {
        assert!(s.expect(req_id, &TaskResult::Throttled { req_id, id }));
    }
}

#[test]
fn test_queried_task_w_throttled_tasks() {
    let mut s = ServerThread::new();
    let mut task_id = [0; 6];
    for i in 0..6 {
        task_id[i] = s.create_task(
            [("get_status".into(), "idle".into())].into(),
            [("mark_done".into(), Box::new(|| "done".to_string()) as Box<dyn FnMut() -> String + Send>)].into()
        );
    }

    s.query_task(task_id[0], "get_status");     // req_id: 6
    s.update_task(task_id[1], "mark_done");     // req_id: 7
    s.query_task(task_id[2], "get_status");     // req_id: 8
    s.query_task(task_id[0], "invalid_query");  // req_id: 9

    s.join_listener();

    // 4 extra tasks were throttled: task_id[4] and task_id[5]
    assert!(s.expect(4, &TaskResult::Throttled {
        req_id: 4,
        id: task_id[4],
    }));
    assert!(s.expect(5, &TaskResult::Throttled {
        req_id: 5,
        id: task_id[5],
    }));

    assert!(s.expect(6, &TaskResult::QueryOk {
        req_id: 6,
        id: task_id[0],
        value: "idle".into()
    }));
    assert!(s.expect(7, &TaskResult::UpdateOk {
        req_id: 7,
        id: task_id[1],
        value: "done".into()
    }));
    assert!(s.expect(8, &TaskResult::QueryOk {
        req_id: 8,
        id: task_id[2],
        value: "idle".into()
    }));
    assert!(s.expect(9, &TaskResult::QueryError {
        req_id: 9,
        id: task_id[0],
        msg: "Query ID 'invalid_query' not found".into()
    }));
}

#[test]
fn test_multiple_queries_in_quick_succession() {
    let mut s = ServerThread::new();
    let task_id = s.create_task(
        [("status".into(), "busy".into())].into(),
        HashMap::new(),
    );

    for _ in 0..10 {
        s.query_task(task_id, "status");
    }

    s.join_listener();

    for i in 1..=10 {
        assert!(s.expect(i, &TaskResult::QueryOk {
            req_id: i,
            id: task_id,
            value: "busy".into()
        }));
    }
}

#[test]
fn test_throttling_recovery_after_timeout() {
    let mut s = ServerThread::new();
    let mut task_ids = vec![];

    for _ in 0..MAX_CONCURRENT_TASKS {
        let id = s.create_task([("info".into(), "live".into())].into(), HashMap::new());
        task_ids.push(id);
    }

    // these should be throttled
    let throttled_id_1 = s.create_task([("info".into(), "extra".into())].into(), HashMap::new()); // req_id: 4
    let throttled_id_2 = s.create_task([("info".into(), "extra".into())].into(), HashMap::new()); // req_id: 5

    thread::sleep(Duration::from_secs(TASK_TIMEOUT + 1));

    let retry_id = s.create_task([("info".into(), "retry".into())].into(), HashMap::new()); // req_id: 6
    s.query_task(retry_id, "info"); // req_id: 7

    s.join_listener();

    assert!(s.expect(4, &TaskResult::Throttled {
        req_id: 4,
        id: throttled_id_1
    }));
    assert!(s.expect(5, &TaskResult::Throttled {
        req_id: 5,
        id: throttled_id_2
    }));
    assert!(s.expect(7, &TaskResult::QueryOk {
        req_id: 7,
        id: retry_id,
        value: "retry".into()
    }));
}
