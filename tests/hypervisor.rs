use server_worker_sim::*;
use std::thread;
use std::time::Duration;

#[test]
fn test_successful_task_execution() {
    let hypervisor = Hypervisor::new();
    hypervisor.create_task("1a", vec![10, 20]);
    hypervisor.listen_for_results();
}

#[test]
fn test_throttling_rejection() {
    let hypervisor = Hypervisor::new();
    for i in 0..MAX_CONCURRENT_TASKS {
        hypervisor.create_task("3", vec![i as i32]);
    }
    hypervisor.create_task("2", vec![99]);
    // thread::sleep(Duration::from_secs(3));
    hypervisor.listen_for_results();
}

#[test]
fn test_invalid_script() {
    let hypervisor = Hypervisor::new();
    hypervisor.create_task("1a!", vec![5, 15]);
    hypervisor.listen_for_results();
}

#[test]
fn test_multiple_queries_and_updates() {
    let hypervisor = Hypervisor::new();
    let task_id = 0;
    hypervisor.create_task("1a1a", vec![10, 20]);
    hypervisor.update_task(task_id, 0, 99);
    hypervisor.query_task(task_id, 0);
    hypervisor.update_task(task_id, 1, 123);
    hypervisor.query_task(task_id, 1);
    hypervisor.listen_for_results();
}