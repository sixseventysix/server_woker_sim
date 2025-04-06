use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc::{self, Sender, Receiver}};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

type TaskId = usize;
type RequestId = usize;

pub struct Task {
    pub id: usize,
    pub query_map: HashMap<String, String>,
    pub update_map: HashMap<String, Box<dyn FnMut() + Send>>,
}

#[derive(Debug)]
pub enum TaskResult {
    QueryOk { req_id: RequestId, id: TaskId, value: String },
    QueryError { req_id: RequestId, id: TaskId, msg: String },
    UpdateOk { req_id: RequestId, id: TaskId },
    UpdateError { req_id: RequestId, id: TaskId, msg: String },
    NotFound { req_id: RequestId, id: TaskId, ctx: &'static str },
}

pub enum Message {
    CreateTask {
        req_id: RequestId,
        id: TaskId,
        query_map: HashMap<String, String>,
        update_map: HashMap<String, Box<dyn FnMut() + Send>>,
        result_tx: Sender<TaskResult>,
    },
    QueryTask {
        req_id: RequestId,
        id: TaskId,
        query_id: String,
        result_tx: Sender<TaskResult>,
    },
    UpdateTask {
        req_id: RequestId,
        id: TaskId,
        update_id: String,
        result_tx: Sender<TaskResult>,
    },
}

#[derive(Debug)]
pub enum TaskControl {
    Query {
        req_id: usize,
        query_id: String,
        result_tx: Sender<TaskResult>,
    },
    Update {
        req_id: usize,
        update_id: String,
        result_tx: Sender<TaskResult>,
    },
}

pub struct Hypervisor {
    pub worker_tx: Sender<Message>,
    pub result_tx: mpsc::Sender<TaskResult>,
    pub request_counter: AtomicUsize,
    pub task_id_counter: AtomicUsize,
    pub listener_handle: Option<JoinHandle<()>>,
}

impl Hypervisor {
    pub fn new() -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        start_worker(worker_rx);

        let listener_handle = thread::spawn(move || {
            while let Ok(result) = result_rx.recv() {
                match result {
                    TaskResult::QueryOk { req_id, id, value } => {
                        println!("[req:{req_id}] Query result for task {id}: {value}");
                    }
                    TaskResult::QueryError { req_id, id, msg } => {
                        println!("[req:{req_id}] Query failed for task {id}: {msg}");
                    }
                    TaskResult::UpdateOk { req_id, id } => {
                        println!("[req:{req_id}] Update OK for task {id}");
                    }
                    TaskResult::UpdateError { req_id, id, msg } => {
                        println!("[req:{req_id}] Update failed for task {id}: {msg}");
                    }
                    TaskResult::NotFound { req_id, id, ctx } => {
                        println!("[req:{req_id}] Task {id} not found: {ctx}");
                    }
                }
            }
        });

        Self {
            worker_tx,
            result_tx: result_tx.clone(),
            request_counter: AtomicUsize::new(0),
            task_id_counter: AtomicUsize::new(0),
            listener_handle: Some(listener_handle)
        }
    }

    pub fn next_req_id(&self) -> RequestId {
        self.request_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub fn next_task_id(&self) -> TaskId {
        self.task_id_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub fn create_task(
        &self,
        query_map: HashMap<String, String>,
        update_map: HashMap<String, Box<dyn FnMut() + Send>>,
    ) -> TaskId {
        let req_id = self.next_req_id();
        let id = self.next_task_id();
        println!("[req:{req_id}] [Hypervisor] Sending create task to worker for Task {id}");
        self.worker_tx
            .send(Message::CreateTask {
                req_id,
                id,
                query_map,
                update_map,
                result_tx: self.result_tx.clone(),
            })
            .unwrap();

        id
    }

    pub fn query_task(&self, id: TaskId, query_id: &str) {
        let req_id = self.next_req_id();
        self.worker_tx
            .send(Message::QueryTask {
                req_id,
                id,
                query_id: query_id.to_string(),
                result_tx: self.result_tx.clone(),
            })
            .unwrap();
    }

    pub fn update_task(&self, id: TaskId, update_id: &str) {
        let req_id = self.next_req_id();
        self.worker_tx
            .send(Message::UpdateTask {
                req_id,
                id,
                update_id: update_id.to_string(),
                result_tx: self.result_tx.clone(),
            })
            .unwrap();
    }
}

fn start_worker(rx: Receiver<Message>) {
    let task_map: Arc<Mutex<HashMap<TaskId, Sender<TaskControl>>>> = Arc::new(Mutex::new(HashMap::new()));
    let active_tasks = Arc::new(AtomicUsize::new(0));
    
    thread::spawn(move || {
        for msg in rx {
            match msg {
                Message::CreateTask {
                    req_id,
                    id,
                    query_map,
                    update_map,
                    result_tx,
                } => {
                    let (task_tx, task_rx) = mpsc::channel();

                    let task = Task {
                        id,
                        query_map,
                        update_map,
                    };

                    task_map.lock().unwrap().insert(id, task_tx.clone());
                    println!("[req:{req_id}] [Worker] Initializing task thread for Task {id}");
                    thread::spawn(move || {
                        task_loop(task, task_rx);
                    });
                }

                Message::QueryTask {
                    req_id,
                    id,
                    query_id,
                    result_tx,
                } => {
                    if let Some(tx) = task_map.lock().unwrap().get(&id) {
                        tx.send(TaskControl::Query {
                            req_id,
                            query_id,
                            result_tx,
                        }).ok();
                    } else {
                        let _ = result_tx.send(TaskResult::NotFound {
                            req_id,
                            id,
                            ctx: "Task not found for query",
                        });
                    }
                }

                Message::UpdateTask {
                    req_id,
                    id,
                    update_id,
                    result_tx,
                } => {
                    if let Some(tx) = task_map.lock().unwrap().get(&id) {
                        tx.send(TaskControl::Update {
                            req_id,
                            update_id,
                            result_tx,
                        }).ok();
                    } else {
                        let _ = result_tx.send(TaskResult::NotFound {
                            req_id,
                            id,
                            ctx: "Task not found for update",
                        });
                    }
                }
            }
        }
    });
}

fn task_loop(mut task: Task, rx: mpsc::Receiver<TaskControl>) {
    while let Ok(msg) = rx.recv() {
        println!("[Task {}] Received control message: {:?}", task.id, msg);
        match msg {
            TaskControl::Query { req_id, query_id, result_tx } => {
                match task.query_map.get(&query_id) {
                    Some(value) => {
                        let _ = result_tx.send(TaskResult::QueryOk {
                            req_id,
                            id: task.id,
                            value: value.clone(),
                        });
                    }
                    None => {
                        let _ = result_tx.send(TaskResult::QueryError {
                            req_id,
                            id: task.id,
                            msg: format!("Query ID '{}' not found", query_id),
                        });
                    }
                }
            }
            TaskControl::Update { req_id, update_id, result_tx } => {
                if let Some(update_fn) = task.update_map.get_mut(&update_id) {
                    update_fn();
                    let _ = result_tx.send(TaskResult::UpdateOk {
                        req_id,
                        id: task.id,
                    });
                } else {
                    let _ = result_tx.send(TaskResult::UpdateError {
                        req_id,
                        id: task.id,
                        msg: format!("Update ID '{update_id}' not found"),
                    });
                }                
            }
        }
    }
}

impl Hypervisor {
    pub fn join_listener(self) {
        if let Some(handle) = self.listener_handle {
            let _ = handle.join();
        }
    }
}
