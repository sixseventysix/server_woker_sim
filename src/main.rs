use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

// --- Task and Message Types ---

type TaskId = usize;

static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

fn generate_id() -> TaskId {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
struct TaskState {
    variables: Vec<i32>,
    timer: u64,
}

#[derive(Debug)]
enum Message {
    Run,
    UpdateVar { index: usize, new_value: i32 },
    Shutdown,
}

#[derive(Debug)]
struct WorkerResult {
    id: TaskId,
    variables: Vec<i32>,
}


struct Hypervisor {
    registry: Arc<Mutex<HashMap<TaskId, mpsc::Sender<Message>>>>,
    result_rx: mpsc::Receiver<WorkerResult>,
}

impl Hypervisor {
    fn new() -> Self {
        let (_tx, rx) = mpsc::channel(); // dummy init
        Self {
            registry: Arc::new(Mutex::new(HashMap::new())),
            result_rx: rx,
        }
    }

    fn with_result_channel() -> (Self, mpsc::Sender<WorkerResult>) {
        let (tx, rx) = mpsc::channel();
        (
            Self {
                registry: Arc::new(Mutex::new(HashMap::new())),
                result_rx: rx,
            },
            tx,
        )
    }

    fn launch_task(&self, timer: u64, variables: Vec<i32>, result_tx: mpsc::Sender<WorkerResult>) -> TaskId {
        let (tx, rx) = mpsc::channel();
        let id = generate_id();

        {
            let mut reg = self.registry.lock().unwrap();
            reg.insert(id, tx.clone());
        }

        let state = TaskState { timer, variables };
        println!("[Hypervisor] Launching task {id}");

        thread::spawn(move || worker_loop(id, state, rx, result_tx));

        tx.send(Message::Run).unwrap();
        id
    }

    fn send_update(&self, task_id: TaskId, index: usize, new_value: i32) {
        if let Some(sender) = self.registry.lock().unwrap().get(&task_id) {
            sender.send(Message::UpdateVar { index, new_value }).unwrap();
            println!("[Hypervisor] Sent update to task {task_id}");
        }
    }

    fn listen_for_results(&self) {
        for result in &self.result_rx {
            println!("[Hypervisor] Worker {} returned {:?}", result.id, result.variables);
        }
    }

    fn shutdown_task(&self, task_id: TaskId) {
        if let Some(sender) = self.registry.lock().unwrap().remove(&task_id) {
            let _ = sender.send(Message::Shutdown);
        }
    }
    
}

fn worker_loop(
    id: TaskId,
    mut state: TaskState,
    rx: mpsc::Receiver<Message>,
    result_tx: mpsc::Sender<WorkerResult>,
) {
    println!("[Worker {id}] Ready with vars = {:?}", state.variables);

    for msg in rx {
        match msg {
            Message::Run => {
                println!("[Worker {id}] Sleeping {}s", state.timer);
                thread::sleep(Duration::from_secs(state.timer));

                for (i, val) in state.variables.iter_mut().enumerate() {
                    println!("  [Worker {id}] var[{i}] before = {}", val);
                    *val += 1;
                    println!("  [Worker {id}] var[{i}] after = {}", val);
                }

                println!("[Worker {id}] Task completed.");
            }

            Message::UpdateVar { index, new_value } => {
                if let Some(var) = state.variables.get_mut(index) {
                    println!("[Worker {id}] Updating var[{index}] to {new_value}");
                    *var = new_value;
                }
            }

            Message::Shutdown => {
                println!("[Worker {id}] Exiting. Returning results.");
                let _ = result_tx.send(WorkerResult {
                    id,
                    variables: state.variables,
                });
                break;
            }
        }
    }
}

fn main() {
    let (hypervisor, result_tx) = Hypervisor::with_result_channel();

    let task_id = hypervisor.launch_task(3, vec![10, 20], result_tx.clone());

    thread::sleep(Duration::from_secs(1));
    hypervisor.send_update(task_id, 0, 42);

    thread::sleep(Duration::from_secs(2));
    hypervisor.shutdown_task(task_id);

    // Drop the last sender — this will close the result channel
    drop(result_tx);

    // This thread will exit cleanly after receiving all worker results
    let result_listener = thread::spawn(move || {
        hypervisor.listen_for_results();
    });

    result_listener.join().unwrap();
}


