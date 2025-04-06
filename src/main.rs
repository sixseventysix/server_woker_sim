use server_worker_sim::*;

fn main() {
    let hypervisor = Hypervisor::new();

    let mut task_id = [0; 6];
    for i in 0..6 {
        task_id[i] = hypervisor.create_task(
            [("get_status".into(), "idle".into())].into(),
            [("mark_done".into(), Box::new(|| println!("Marked done")) as Box<dyn FnMut() + Send>)].into()
        );
    }

    hypervisor.query_task(task_id[0], "get_status");
    hypervisor.update_task(task_id[0], "mark_done");
    hypervisor.query_task(task_id[0], "invalid_query");
    hypervisor.join_listener();
}