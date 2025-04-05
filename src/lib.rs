use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Arc, Mutex, atomic::Ordering, atomic::AtomicUsize};
use std::thread;
use std::time::Duration;
use std::io::{self, Write};

const MAX_CONCURRENT_TASKS: usize = 4;
type TaskId = usize;

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        let mut stdout = io::stdout();
        writeln!(stdout, $($arg)*).unwrap();
        stdout.flush().unwrap();
    }};
}

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
        result_tx: mpsc::Sender<TaskResult>,
    },
    UpdateTask {
        id: TaskId,
        idx: usize,
        new_value: i32,
        result_tx: mpsc::Sender<TaskResult>,
    },
    QueryTask {
        id: TaskId,
        idx: usize,
        result_tx: mpsc::Sender<TaskResult>,
    },
}

#[derive(Debug)]
enum TaskControl {
    UpdateVar { idx: usize, new_value: i32, result_tx: mpsc::Sender<TaskResult> },
    QueryVar { idx: usize, result_tx: mpsc::Sender<TaskResult> },
}

#[derive(Debug)]
pub struct Task {
    pub id: TaskId,
    pub variables: Vec<i32>,
    pub steps: VecDeque<ProcessStep>,
}

#[derive(Debug)]
pub enum TaskResult {
    Success(TaskId, Vec<i32>),
    InitError(TaskId, String),
    Throttled(TaskId),
    NotFound(TaskId, &'static str),
    QueryResult(TaskId, Result<i32, usize>),
    UpdateResult(TaskId, Result<(), usize>),
}

pub fn task_init(id: TaskId, script: &str, variables: Vec<i32>) -> Result<Task, String> {
    let mut steps = VecDeque::new();
    let mut chars = script.chars().peekable();

    if script.trim().is_empty() {
        return Err("Empty script".into());
    }

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
        } else {
            return Err(format!("Unexpected character '{}'", ch));
        }
    }

    Ok(Task { id, variables, steps })
}

fn execute_next_step(task: &mut Task) -> bool {
    if let Some(step) = task.steps.pop_front() {
        match step {
            ProcessStep::Sleep(secs) => {
                log!("[Task {}] Sleeping {}s", task.id, secs);
                thread::sleep(Duration::from_secs(secs));
            }
            ProcessStep::Arithmetic => {
                for (i, val) in task.variables.iter_mut().enumerate() {
                    log!("  [Task {}] var[{i}] before = {}", task.id, val);
                    *val += 1;
                    log!("  [Task {}] var[{i}] after = {}", task.id, val);
                }
            }
        }
        true
    } else {
        false
    }
}

fn task_loop(mut task: Task, rx: mpsc::Receiver<TaskControl>, result_tx: mpsc::Sender<TaskResult>) {
    loop {
        let has_more = execute_next_step(&mut task);

        while let Ok(msg) = rx.try_recv() {
            match msg {
                TaskControl::QueryVar { idx, result_tx } => {
                    let result = task.variables.get(idx)
                        .cloned()
                        .ok_or(idx);
                    let _ = result_tx.send(TaskResult::QueryResult(task.id, result));
                }
                TaskControl::UpdateVar { idx, new_value, result_tx } => {
                    let result = match task.variables.get_mut(idx) {
                        Some(var) => {
                            *var = new_value;
                            Ok(())
                        }
                        None => Err(idx),
                    };
                    let _ = result_tx.send(TaskResult::UpdateResult(task.id, result));
                }
            }
        }

        if !has_more {
            log!("[Task {}] Finished. Final vars: {:?}", task.id, task.variables);
            let _ = result_tx.send(TaskResult::Success(task.id, task.variables.clone()));
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
                    if task_counter.load(Ordering::SeqCst) >= MAX_CONCURRENT_TASKS {
                        log!("[Worker] Task {id} rejected due to throttling");
                        let _ = result_tx.send(TaskResult::Throttled(id));
                        continue;
                    }

                    match task_init(id, &script, variables) {
                        Ok(task) => { 
                            let (task_tx, task_rx) = mpsc::channel();
                            task_map.lock().unwrap().insert(id, task_tx);
                            let task_map_cloned = Arc::clone(&task_map);

                            thread::spawn(move || {
                                task_loop(task, task_rx, result_tx);
                                task_map_cloned.lock().unwrap().remove(&id);
                            });

                            log!("[Worker] Task {id} created");
                        }
                        Err(msg) => {
                            log!("[Worker] Task {id} failed to initialize: {msg}");
                            let _ = result_tx.send(TaskResult::InitError(id, msg));
                        }
                    }
                }

                Message::UpdateTask { id, idx, new_value, result_tx } => {
                    if let Some(tx) = task_map.lock().unwrap().get(&id) {
                        tx.send(TaskControl::UpdateVar { idx, new_value, result_tx }).ok();
                    } else {
                        let _ = result_tx.send(TaskResult::NotFound(id, "Update target not found"));
                    }
                }

                Message::QueryTask { id, idx, result_tx } => {
                    if let Some(tx) = task_map.lock().unwrap().get(&id) {
                        tx.send(TaskControl::QueryVar { idx, result_tx }).ok();
                    } else {
                        let _ = result_tx.send(TaskResult::NotFound(id, "Query target not found"));
                    }
                }
            }
        }
    });
}

pub struct Hypervisor {
    worker_tx: mpsc::Sender<Message>,
    result_tx: mpsc::Sender<TaskResult>,
    result_rx: mpsc::Receiver<TaskResult>,
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
    pub fn create_task(&self, id: TaskId, script: &str, vars: Vec<i32>) {
        self.task_counter.fetch_add(1, Ordering::SeqCst);
        self.worker_tx.send(Message::CreateTask {
            id,
            script: script.to_string(),
            variables: vars,
            result_tx: self.result_tx.clone(),
        }).unwrap();
    
        log!("[Hypervisor] Created task {id}");
    }
    

    pub fn update_task(&self, id: TaskId, idx: usize, new_value: i32) {
        self.worker_tx
            .send(Message::UpdateTask {
                id,
                idx,
                new_value,
                result_tx: self.result_tx.clone(),
            })
            .unwrap();

        log!("[Hypervisor] Sent update to task {id}: var[{idx}] = {new_value}");
    }

    pub fn query_task(&self, id: TaskId, idx: usize) {
        self.worker_tx
            .send(Message::QueryTask {
                id,
                idx,
                result_tx: self.result_tx.clone(),
            })
            .unwrap();
    
        log!("[Hypervisor] Sent query to task {id} for var[{idx}]");
    }    

    pub fn listen_for_results(&self) {
        while self.task_counter.load(Ordering::SeqCst) > 0 {
            if let Ok(result) = self.result_rx.recv() {
                match result {
                    TaskResult::Success(id, vars) => {
                        log!("[Hypervisor] Task {id} completed with variables: {:?}", vars);
                        self.task_counter.fetch_sub(1, Ordering::SeqCst);
                    }
                    TaskResult::InitError(id, msg) => {
                        log!("[Hypervisor] Task {id} failed to initialize: {msg}");
                        self.task_counter.fetch_sub(1, Ordering::SeqCst);
                    }
                    TaskResult::Throttled(id) => {
                        log!("[Hypervisor] Task {id} was throttled");
                    }
                    TaskResult::NotFound(id, context) => {
                        log!("[Hypervisor] Task {id} not found: {context}");
                    }
                    TaskResult::QueryResult(id, Ok(val)) => {
                        log!("[Hypervisor] Query result for task {id}: {val}");
                    }
                    TaskResult::QueryResult(id, Err(bad)) => {
                        log!("[Hypervisor] Query failed for task {id}: invalid idx {bad}");
                    }
                    TaskResult::UpdateResult(id, Ok(())) => {
                        log!("[Hypervisor] Updated successfully for task {id}");
                    }
                    TaskResult::UpdateResult(id, Err(bad)) => {
                        log!("[Hypervisor] Update failed for task {id}: invalid idx {bad}");
                    }
                }
            }
        }
    
        log!("[Hypervisor] All tasks completed. Exiting.");
    }    
    
}

