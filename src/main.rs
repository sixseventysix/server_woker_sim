use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

type TaskId = usize;

#[derive(Debug)]
enum ProcessStep {
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
struct Task {
    id: TaskId,
    variables: Vec<i32>,
    steps: VecDeque<ProcessStep>,
}

fn parse_script(id: TaskId, script: &str, variables: Vec<i32>) -> Task {
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


fn start_worker(rx: mpsc::Receiver<Message>) {
    let task_map = Arc::new(Mutex::new(HashMap::new()));

    thread::spawn(move || {
        for msg in rx {
            match msg {
                Message::CreateTask { id, script, variables, result_tx } => {
                    let task = parse_script(id, &script, variables);
                    let (task_tx, task_rx) = mpsc::channel();
                    task_map.lock().unwrap().insert(id, task_tx);

                    thread::spawn(move || {
                        task_loop(task, task_rx, result_tx);
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

fn main() {
    let (tx, rx) = mpsc::channel();
    start_worker(rx);

    let task_id = 1;
    let (result_tx, result_rx) = mpsc::channel();

    tx.send(Message::CreateTask {
        id: task_id,
        script: "2a1a".into(),
        variables: vec![10, 20],
        result_tx,
    }).unwrap();

    thread::sleep(Duration::from_secs(1));
    tx.send(Message::UpdateTask {
        id: task_id,
        index: 0,
        new_value: 99,
    }).unwrap();

    thread::sleep(Duration::from_secs(2));

    let (resp_tx, resp_rx) = mpsc::channel();
    tx.send(Message::QueryTask {
        id: task_id,
        response_tx: resp_tx,
    }).unwrap();

    if let Ok(vars) = resp_rx.recv() {
        println!("[Server] Query response for Task {task_id}: {:?}", vars);
    }

    if let Ok((tid, final_vars)) = result_rx.recv() {
        println!("[Server] Task {tid} completed with final variables: {:?}", final_vars);
    }

    thread::sleep(Duration::from_secs(1));
}

