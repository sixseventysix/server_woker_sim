use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Arc, Mutex, atomic::Ordering, atomic::AtomicUsize};
use std::thread;
use std::time::Duration;

const MAX_CONCURRENT_TASKS: usize = 4;

type TaskId = usize;

#[derive(Debug, PartialEq)]
pub enum ProcessStep {
    Sleep(u64),
    Arithmetic,
}

#[derive(Debug)]
enum Message {
    CreateTask {
        id: TaskId,
        script: String,
        variables: Vec<i32>,
        result_tx: mpsc::Sender<(TaskId, Vec<i32>)>,
    },
    UpdateTask {
        id: TaskId,
        index: usize,
        new_value: i32,
    },
    QueryTask {
        id: TaskId,
        response_tx: mpsc::Sender<Vec<i32>>,
    },
}

#[derive(Debug)]
enum TaskControl {
    UpdateVar { index: usize, new_value: i32 },
    QueryVar { response_tx: mpsc::Sender<Vec<i32>> },
}

#[derive(Debug)]
pub struct Task {
    pub id: TaskId,
    pub variables: Vec<i32>,
    pub steps: VecDeque<ProcessStep>,
}

pub fn task_init(id: TaskId, script: &str, variables: Vec<i32>) -> Task {
    let mut steps = VecDeque::new();
    let mut chars = script.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch.is_digit(10) {
            let mut num = ch.to_digit(10).unwrap() as u64;
            while let Some(next_ch) = chars.peek() {
                if next_ch.is_digit(10) {
                    num = num * 10 + next_ch.to_digit(10).unwrap() as u64;
                    chars.next();
                } else {
                    break;
                }
            }
            steps.push_back(ProcessStep::Sleep(num));
        } else if ch == 'a' {
            steps.push_back(ProcessStep::Arithmetic);
        }
    }

    Task { id, variables, steps }
}

fn execute_next_step(task: &mut Task) -> bool {
    if let Some(step) = task.steps.pop_front() {
        match step {
            ProcessStep::Sleep(secs) => {
                println!("[Task {}] Sleeping {}s", task.id, secs);
                thread::sleep(Duration::from_secs(secs));
            }
            ProcessStep::Arithmetic => {
                for (i, val) in task.variables.iter_mut().enumerate() {
                    println!("  [Task {}] var[{i}] before = {}", task.id, val);
                    *val += 1;
                    println!("  [Task {}] var[{i}] after = {}", task.id, val);
                }
            }
        }
        true
    } else {
        false
    }
}

fn task_loop(mut task: Task, rx: mpsc::Receiver<TaskControl>, result_tx: mpsc::Sender<(TaskId, Vec<i32>)>) {
    loop {
        let has_more = execute_next_step(&mut task);

        while let Ok(msg) = rx.try_recv() {
            match msg {
                TaskControl::UpdateVar { index, new_value } => {
                    if let Some(var) = task.variables.get_mut(index) {
                        println!("[Task {}] Updating var[{index}] to {}", task.id, new_value);
                        *var = new_value;
                    }
                }
                TaskControl::QueryVar { response_tx } => {
                    let _ = response_tx.send(task.variables.clone());
                }
            }
        }

        if !has_more {
            println!("[Task {}] Finished. Final vars: {:?}", task.id, task.variables);
            let _ = result_tx.send((task.id, task.variables.clone()));
            break;
        }
    }
}


fn start_worker(rx: mpsc::Receiver<Message>, task_counter: Arc<AtomicUsize>) {
    let task_map = Arc::new(Mutex::new(HashMap::new()));

    thread::spawn(move || {
        for msg in rx {
            match msg {
                Message::CreateTask { id, script, variables, result_tx } => {
                    task_counter.fetch_add(1, Ordering::SeqCst);

                    let task = task_init(id, &script, variables);
                    let (task_tx, task_rx) = mpsc::channel();
                    task_map.lock().unwrap().insert(id, task_tx);

                    let tc = Arc::clone(&task_counter);

                    thread::spawn(move || {
                        task_loop(task, task_rx, result_tx);
                        tc.fetch_sub(1, Ordering::SeqCst);
                    });

                    println!("[Worker] Task {id} created");
                }

                Message::UpdateTask { id, index, new_value } => {
                    if let Some(tx) = task_map.lock().unwrap().get(&id) {
                        tx.send(TaskControl::UpdateVar { index, new_value }).ok();
                    }
                }

                Message::QueryTask { id, response_tx } => {
                    if let Some(tx) = task_map.lock().unwrap().get(&id) {
                        tx.send(TaskControl::QueryVar { response_tx }).ok();
                    }
                }
            }
        }
    });
}

pub struct Hypervisor {
    worker_tx: mpsc::Sender<Message>,
    result_tx: mpsc::Sender<(TaskId, Vec<i32>)>,
    result_rx: mpsc::Receiver<(TaskId, Vec<i32>)>,
    task_counter: Arc<AtomicUsize>,
}

impl Hypervisor {
    pub fn new() -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let task_counter = Arc::new(AtomicUsize::new(0));
        start_worker(worker_rx, Arc::clone(&task_counter));
        Self {
            worker_tx,
            result_tx,
            result_rx,
            task_counter
        }
    }
}


impl Hypervisor {
    fn create_task(&self, id: TaskId, script: &str, vars: Vec<i32>) {
        self.worker_tx.send(Message::CreateTask {
            id,
            script: script.to_string(),
            variables: vars,
            result_tx: self.result_tx.clone(),
        }).unwrap();
    
        println!("[Hypervisor] Created task {id}");
    }
    

    fn update_task(&self, id: TaskId, index: usize, new_value: i32) {
        self.worker_tx.send(Message::UpdateTask {
            id,
            index,
            new_value,
        }).unwrap();

        println!("[Hypervisor] Sent update to task {id}: var[{index}] = {new_value}");
    }

    fn query_task(&self, id: TaskId) {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.worker_tx.send(Message::QueryTask {
            id,
            response_tx: resp_tx,
        }).unwrap();

        if let Ok(vars) = resp_rx.recv() {
            println!("[Hypervisor] Query result for task {id}: {:?}", vars);
        }
    }

    fn listen_for_results(&self) {
        while self.task_counter.load(Ordering::SeqCst) > 0 {
            if let Ok((id, vars)) = self.result_rx.recv() {
                println!("[Hypervisor] Task {id} completed with variables: {:?}", vars);
            }
        }
    
        println!("[Hypervisor] All tasks completed. Exiting.");
    }
    
}

