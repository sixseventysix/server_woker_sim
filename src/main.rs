use server_worker_sim::*;

fn main() {
    let hypervisor = Hypervisor::new();

    let task_id = hypervisor.create_task(
        [("get_status".into(), "idle".into())].into(),
        [("mark_done".into(), Box::new(|| println!("Marked done")) as Box<dyn FnMut() + Send>)].into()
    );

    hypervisor.query_task(task_id, "get_status");
    hypervisor.update_task(task_id, "mark_done");
    hypervisor.query_task(task_id, "invalid_query");
    hypervisor.join_listener();
}