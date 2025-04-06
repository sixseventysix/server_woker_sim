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
        req_id: usize,
        id: TaskId,
        script: String,
        variables: Vec<i32>,
        result_tx: mpsc::Sender<TaskResult>,
    },
    UpdateTask {
        req_id: usize,
        id: TaskId,
        idx: usize,
        new_value: i32,
        result_tx: mpsc::Sender<TaskResult>,
    },
    QueryTask {
        req_id: usize,
        id: TaskId,
        idx: usize,
        result_tx: mpsc::Sender<TaskResult>,
    },
}

#[derive(Debug)]
enum TaskControl {
    UpdateVar { 
        req_id: usize, 
        idx: usize, 
        new_value: i32, 
        result_tx: mpsc::Sender<TaskResult> 
    },
    QueryVar { 
        req_id: usize, 
        idx: usize, 
        result_tx: mpsc::Sender<TaskResult> 
    }
}

#[derive(Debug)]
pub struct Task {
    pub id: TaskId,
    pub variables: Vec<i32>,
    pub steps: VecDeque<ProcessStep>,
}

#[derive(Debug)]
pub enum TaskResult {
    Success { req_id: usize, id: TaskId, variables: Vec<i32> },
    InitError { req_id: usize, id: TaskId, msg: String },
    Throttled { req_id: usize, id: TaskId },
    NotFound { req_id: usize, id: TaskId, ctx: &'static str },
    QueryResult { req_id: usize, id: TaskId, result: Result<i32, usize> },
    UpdateResult { req_id: usize, id: TaskId, result: Result<(), usize> },
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

fn task_loop(req_id: usize, mut task: Task, rx: mpsc::Receiver<TaskControl>, result_tx: mpsc::Sender<TaskResult>) {
    loop {
        let has_more = execute_next_step(&mut task);

        while let Ok(msg) = rx.try_recv() {
            match msg {
                TaskControl::QueryVar { req_id, idx, result_tx } => {
                    let result = task.variables.get(idx).cloned().ok_or(idx);
                    let _ = result_tx.send(TaskResult::QueryResult { req_id, id: task.id, result });
                }
                TaskControl::UpdateVar { req_id, idx, new_value, result_tx } => {
                    let result = match task.variables.get_mut(idx) {
                        Some(var) => {
                            *var = new_value;
                            Ok(())
                        }
                        None => Err(idx),
                    };
                    let _ = result_tx.send(TaskResult::UpdateResult { req_id, id: task.id, result });
                }
            }
        }

        if !has_more {
            log!("[Task {}] Finished. Final vars: {:?}", task.id, task.variables);
            let _ = result_tx.send(TaskResult::Success { req_id, id: task.id, variables: task.variables.clone() });
            break;
        }
    }
}

fn start_worker(rx: mpsc::Receiver<Message>, task_counter: Arc<AtomicUsize>) {
    let task_map = Arc::new(Mutex::new(HashMap::new()));

    thread::spawn(move || {
        for msg in rx {
            match msg {
                Message::CreateTask { req_id, id, script, variables, result_tx } => {
                    if task_counter.load(Ordering::SeqCst) >= MAX_CONCURRENT_TASKS {
                        log!("[req:{req_id}] [Worker] Task {id} rejected due to throttling");
                        let _ = result_tx.send(TaskResult::Throttled { req_id, id });
                        continue;
                    }

                    match task_init(id, &script, variables) {
                        Ok(task) => {
                            let (task_tx, task_rx) = mpsc::channel();
                            task_map.lock().unwrap().insert(id, task_tx);
                            let task_map_cloned = Arc::clone(&task_map);

                            thread::spawn(move || {
                                task_loop(req_id, task, task_rx, result_tx);
                                task_map_cloned.lock().unwrap().remove(&id);
                            });

                            log!("[req:{req_id}] [Worker] Task {id} created");
                        }
                        Err(msg) => {
                            log!("[req:{req_id}] [Worker] Task {id} failed to initialize: {msg}");
                            let _ = result_tx.send(TaskResult::InitError { req_id, id, msg });
                        }
                    }
                }

                Message::UpdateTask { req_id, id, idx, new_value, result_tx } => {
                    if let Some(tx) = task_map.lock().unwrap().get(&id) {
                        tx.send(TaskControl::UpdateVar { req_id, idx, new_value, result_tx }).ok();
                    } else {
                        let _ = result_tx.send(TaskResult::NotFound { req_id, id, ctx: "Update target not found" });
                    }
                }

                Message::QueryTask { req_id, id, idx, result_tx } => {
                    if let Some(tx) = task_map.lock().unwrap().get(&id) {
                        tx.send(TaskControl::QueryVar { req_id, idx, result_tx }).ok();
                    } else {
                        let _ = result_tx.send(TaskResult::NotFound { req_id, id, ctx: "Query target not found" });
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
    task_id_counter: AtomicUsize,
    request_counter: AtomicUsize,
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
            task_counter,
            task_id_counter: AtomicUsize::new(0),
            request_counter: AtomicUsize::new(0),
        }
    }
}

impl Hypervisor {
    pub fn next_req_id(&self) -> usize {
        self.request_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub fn next_task_id(&self) -> usize {
        self.task_id_counter.fetch_add(1, Ordering::SeqCst)
    }    

    pub fn create_task(&self, script: &str, vars: Vec<i32>) {
        let req_id = self.next_req_id();
        let id = self.next_task_id();
        self.task_counter.fetch_add(1, Ordering::SeqCst);
        self.worker_tx.send(Message::CreateTask {
            req_id,
            id,
            script: script.to_string(),
            variables: vars,
            result_tx: self.result_tx.clone(),
        }).unwrap();
    
        log!("[req:{req_id}] [Hypervisor] Created task {id}");
    }
    

    pub fn update_task(&self, id: TaskId, idx: usize, new_value: i32) {
        let req_id = self.next_req_id();
        self.worker_tx
            .send(Message::UpdateTask {
                req_id,
                id,
                idx,
                new_value,
                result_tx: self.result_tx.clone(),
            })
            .unwrap();

        log!("[req:{req_id}] [Hypervisor] Sent update to task {id}: var[{idx}] = {new_value}");
    }

    pub fn query_task(&self, id: TaskId, idx: usize) {
        let req_id = self.next_req_id();
        self.worker_tx
            .send(Message::QueryTask {
                req_id,
                id,
                idx,
                result_tx: self.result_tx.clone(),
            })        
            .unwrap();
    
        log!("[req:{req_id}] [Hypervisor] Sent query to task {id} for var[{idx}]");
    }    

    pub fn listen_for_results(&self) {
        while self.task_counter.load(Ordering::SeqCst) > 0 {
            if let Ok(result) = self.result_rx.recv() {
                match result {
                    TaskResult::Success { req_id, id, variables } => {
                        log!("[req:{req_id}] Task {id} completed with variables: {:?}", variables);
                        self.task_counter.fetch_sub(1, Ordering::SeqCst);
                    }
    
                    TaskResult::InitError { req_id, id, msg } => {
                        log!("[req:{req_id}] Task {id} failed to initialize: {msg}");
                        self.task_counter.fetch_sub(1, Ordering::SeqCst);
                    }
    
                    TaskResult::Throttled { req_id, id } => {
                        log!("[req:{req_id}] Task {id} was throttled");
                        self.task_counter.fetch_sub(1, Ordering::SeqCst);
                    }
    
                    TaskResult::NotFound { req_id, id, ctx } => {
                        log!("[req:{req_id}] Task {id} not found: {ctx}");
                        self.task_counter.fetch_sub(1, Ordering::SeqCst);
                    }
    
                    TaskResult::QueryResult { req_id, id, result } => match result {
                        Ok(val) => {
                            log!("[req:{req_id}] Query result for task {id}: {val}");
                        }
                        Err(bad) => {
                            log!("[req:{req_id}] Query failed for task {id}: invalid idx {bad}");
                        }
                    },
    
                    TaskResult::UpdateResult { req_id, id, result } => match result {
                        Ok(()) => {
                            log!("[req:{req_id}] Updated successfully for task {id}");
                        }
                        Err(bad) => {
                            log!("[req:{req_id}] Update failed for task {id}: invalid idx {bad}");
                        }
                    },
                }
            }
        }
    
        log!("[Hypervisor] All tasks completed. Exiting.");
    }     
    
}

