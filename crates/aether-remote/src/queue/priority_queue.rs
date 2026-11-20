//! 内存优先级队列实现

use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use uuid::Uuid;

use super::error::{QueueError, Result};
use super::models::Task;

/// 带优先级的任务包装
#[derive(Debug, Clone)]
struct PrioritizedTask {
    task: Task,
    priority: i32,
    created_at: DateTime<Utc>,
}

impl PartialEq for PrioritizedTask {
    fn eq(&self, other: &Self) -> bool {
        self.task.id == other.task.id
    }
}

impl Eq for PrioritizedTask {}

impl Ord for PrioritizedTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // 优先级高的在前
        self.priority
            .cmp(&other.priority)
            // 优先级相同，创建时间早的在前
            .then_with(|| other.created_at.cmp(&self.created_at))
    }
}

impl PartialOrd for PrioritizedTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 内存优先级队列
pub struct PriorityTaskQueue {
    heap: BinaryHeap<PrioritizedTask>,
    task_map: HashMap<Uuid, Task>,
    max_size: usize,
}

impl PriorityTaskQueue {
    /// 创建新的优先级队列
    pub fn new(max_size: usize) -> Self {
        Self {
            heap: BinaryHeap::new(),
            task_map: HashMap::new(),
            max_size,
        }
    }

    /// 添加任务到队列
    pub fn push(&mut self, task: Task) -> Result<()> {
        if self.heap.len() >= self.max_size {
            return Err(QueueError::QueueFull(self.max_size));
        }

        if self.task_map.contains_key(&task.id) {
            return Err(QueueError::DuplicateTask(task.id.to_string()));
        }

        let pt = PrioritizedTask {
            priority: task.priority,
            created_at: task.created_at,
            task: task.clone(),
        };

        self.heap.push(pt);
        self.task_map.insert(task.id, task);
        Ok(())
    }

    /// 弹出最高优先级任务
    pub fn pop(&mut self) -> Option<Task> {
        let pt = self.heap.pop()?;
        self.task_map.remove(&pt.task.id);
        Some(pt.task)
    }

    /// 根据 ID 弹出任务
    pub fn pop_by_id(&mut self, task_id: Uuid) -> Option<Task> {
        let task = self.task_map.remove(&task_id)?;

        // 重建堆（移除指定任务）
        let tasks: Vec<_> = self
            .heap
            .drain()
            .filter(|pt| pt.task.id != task_id)
            .collect();
        self.heap = tasks.into_iter().collect();

        Some(task)
    }

    /// 查看队首任务（不移除）
    pub fn peek(&self) -> Option<&Task> {
        self.heap.peek().map(|pt| &pt.task)
    }

    /// 获取任务
    pub fn get(&self, task_id: Uuid) -> Option<&Task> {
        self.task_map.get(&task_id)
    }

    /// 检查任务是否存在
    pub fn contains(&self, task_id: Uuid) -> bool {
        self.task_map.contains_key(&task_id)
    }

    /// 队列长度
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.heap.len() >= self.max_size
    }

    /// 清空队列
    pub fn clear(&mut self) {
        self.heap.clear();
        self.task_map.clear();
    }

    /// 获取所有任务
    pub fn tasks(&self) -> Vec<&Task> {
        self.task_map.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_task(priority: i32, title: &str) -> Task {
        let mut task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            title.to_string(),
            "cmd".to_string(),
        );
        task.priority = priority;
        task
    }

    #[test]
    fn test_push_and_pop() {
        let mut queue = PriorityTaskQueue::new(10);

        let task1 = create_task(1, "low");
        let task2 = create_task(5, "high");
        let task3 = create_task(3, "medium");

        queue.push(task1.clone()).unwrap();
        queue.push(task2.clone()).unwrap();
        queue.push(task3.clone()).unwrap();

        assert_eq!(queue.len(), 3);

        // 应该按优先级弹出
        let popped = queue.pop().unwrap();
        assert_eq!(popped.title, "high");

        let popped = queue.pop().unwrap();
        assert_eq!(popped.title, "medium");

        let popped = queue.pop().unwrap();
        assert_eq!(popped.title, "low");

        assert!(queue.is_empty());
    }

    #[test]
    fn test_pop_by_id() {
        let mut queue = PriorityTaskQueue::new(10);

        let task1 = create_task(1, "task1");
        let task2 = create_task(2, "task2");
        let task3 = create_task(3, "task3");

        let id1 = task1.id;
        let id2 = task2.id;

        queue.push(task1).unwrap();
        queue.push(task2).unwrap();
        queue.push(task3).unwrap();

        assert_eq!(queue.len(), 3);

        let popped = queue.pop_by_id(id2).unwrap();
        assert_eq!(popped.id, id2);
        assert_eq!(queue.len(), 2);
        assert!(!queue.contains(id2));
        assert!(queue.contains(id1));
    }

    #[test]
    fn test_queue_full() {
        let mut queue = PriorityTaskQueue::new(2);

        queue.push(create_task(1, "task1")).unwrap();
        queue.push(create_task(2, "task2")).unwrap();

        assert!(queue.is_full());

        let result = queue.push(create_task(3, "task3"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), QueueError::QueueFull(2)));
    }

    #[test]
    fn test_duplicate_task() {
        let mut queue = PriorityTaskQueue::new(10);

        let task = create_task(1, "task");
        queue.push(task.clone()).unwrap();

        let result = queue.push(task);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), QueueError::DuplicateTask(_)));
    }

    #[test]
    fn test_peek() {
        let mut queue = PriorityTaskQueue::new(10);

        let task1 = create_task(1, "low");
        let task2 = create_task(5, "high");

        queue.push(task1).unwrap();
        queue.push(task2).unwrap();

        let peeked = queue.peek().unwrap();
        assert_eq!(peeked.title, "high");
        assert_eq!(queue.len(), 2); // peek 不移除
    }

    #[test]
    fn test_clear() {
        let mut queue = PriorityTaskQueue::new(10);

        queue.push(create_task(1, "task1")).unwrap();
        queue.push(create_task(2, "task2")).unwrap();

        assert_eq!(queue.len(), 2);

        queue.clear();
        assert!(queue.is_empty());
    }
}
