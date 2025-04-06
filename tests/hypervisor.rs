use server_worker_sim::*;
use std::thread;
use std::time::Duration;

#[test]
fn test_task_rejected_after_shutdown() {
    let mut hypervisor = Hypervisor::new();
    hypervisor.create_task("1a", vec![10]);
    hypervisor.create_task("1a", vec![10]);
    hypervisor.shutdown();
    hypervisor.join_listener();
    
}
