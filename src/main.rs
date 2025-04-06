use server_worker_sim::*;

fn main() {
    let s = ServerThread::new();

    let mut task_id = [0; 6];
    for i in 0..6 {
        task_id[i] = s.create_task(
            [("get_status".into(), "idle".into())].into(),
            [("mark_done".into(), Box::new(|| println!("Marked done")) as Box<dyn FnMut() + Send>)].into()
        );
    }

    s.query_task(task_id[0], "get_status");
    s.update_task(task_id[0], "mark_done");
    s.query_task(task_id[0], "invalid_query");
    s.join_listener();
}