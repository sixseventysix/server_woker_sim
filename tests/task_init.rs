use server_worker_sim::{task_init, ProcessStep};
use std::collections::VecDeque;

#[test]
fn test_init_with_single_sleep() {
    let task = task_init(1, "5", vec![1, 2]);
    
    let mut expected_steps = VecDeque::new();
    expected_steps.push_back(ProcessStep::Sleep(5));
    
    assert_eq!(task.id, 1);
    assert_eq!(task.variables, vec![1, 2]);
    assert_eq!(task.steps, expected_steps);
}

#[test]
fn test_init_with_arithmetic_and_sleep() {
    let task = task_init(2, "3a", vec![42]);
    
    let mut expected_steps = VecDeque::new();
    expected_steps.push_back(ProcessStep::Sleep(3));
    expected_steps.push_back(ProcessStep::Arithmetic);

    assert_eq!(task.steps, expected_steps);
}

#[test]
fn test_init_with_multi_digit_sleep() {
    let task = task_init(3, "10a1a", vec![]);
    
    let mut expected_steps = VecDeque::new();
    expected_steps.push_back(ProcessStep::Sleep(10));
    expected_steps.push_back(ProcessStep::Arithmetic);
    expected_steps.push_back(ProcessStep::Sleep(1));
    expected_steps.push_back(ProcessStep::Arithmetic);

    assert_eq!(task.steps, expected_steps);
}

#[test]
fn test_empty_script() {
    let task = task_init(4, "", vec![99]);
    
    let expected_steps: VecDeque<ProcessStep> = VecDeque::new();

    assert_eq!(task.id, 4);
    assert_eq!(task.variables, vec![99]);
    assert_eq!(task.steps, expected_steps);
}

#[test]
fn test_double_a() {
    let task = task_init(4, "5aa", vec![99]);
    
    let mut expected_steps: VecDeque<ProcessStep> = VecDeque::new();
    expected_steps.push_back(ProcessStep::Sleep(5));
    expected_steps.push_back(ProcessStep::Arithmetic);
    expected_steps.push_back(ProcessStep::Arithmetic);

    assert_eq!(task.id, 4);
    assert_eq!(task.variables, vec![99]);
    assert_eq!(task.steps, expected_steps);
}
