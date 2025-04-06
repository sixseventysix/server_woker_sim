use server_worker_sim::*;
use std::thread;
use std::time::Duration;

#[test]
fn test_successful_task_execution() {
    let hypervisor = Hypervisor::new();
    hypervisor.create_task("1a", vec![10, 20]);
    hypervisor.listen_for_results();
}