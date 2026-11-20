//! 任务依赖图管理

use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::HashMap;
use uuid::Uuid;

use super::error::{QueueError, Result};
use super::models::Task;

/// 依赖图
pub struct DependencyGraph {
    graph: DiGraph<Uuid, ()>,
    task_map: HashMap<Uuid, NodeIndex>,
}

impl DependencyGraph {
    /// 创建新的依赖图
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            task_map: HashMap::new(),
        }
    }

    /// 添加任务到依赖图
    pub fn add_task(&mut self, task: &Task) -> Result<()> {
        if self.task_map.contains_key(&task.id) {
            return Err(QueueError::DuplicateTask(task.id.to_string()));
        }

        let node = self.graph.add_node(task.id);
        self.task_map.insert(task.id, node);

        // 添加依赖边
        for dep_id in &task.dependencies {
            if let Some(&dep_node) = self.task_map.get(dep_id) {
                self.graph.add_edge(dep_node, node, ());
            } else {
                // 依赖的任务不存在，可能是外部任务或已完成任务
                // 这里我们选择忽略，实际应用中可能需要更严格的检查
                tracing::warn!("任务 {} 依赖的任务 {} 不存在", task.id, dep_id);
            }
        }

        // 检测循环依赖
        if self.has_cycle() {
            self.remove_task(task.id);
            return Err(QueueError::CircularDependency(task.id.to_string()));
        }

        Ok(())
    }

    /// 移除任务
    pub fn remove_task(&mut self, task_id: Uuid) {
        if let Some(node) = self.task_map.remove(&task_id) {
            self.graph.remove_node(node);
        }
    }

    /// 获取就绪任务（所有依赖已完成）
    pub fn get_ready_tasks(&self) -> Vec<Uuid> {
        // 找出所有入度为 0 的节点
        self.graph
            .node_indices()
            .filter(|&node| {
                self.graph
                    .neighbors_directed(node, Direction::Incoming)
                    .count()
                    == 0
            })
            .map(|node| self.graph[node])
            .collect()
    }

    /// 获取任务的依赖
    pub fn get_dependencies(&self, task_id: Uuid) -> Vec<Uuid> {
        if let Some(&node) = self.task_map.get(&task_id) {
            self.graph
                .neighbors_directed(node, Direction::Incoming)
                .map(|n| self.graph[n])
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 获取任务的依赖者
    pub fn get_dependents(&self, task_id: Uuid) -> Vec<Uuid> {
        if let Some(&node) = self.task_map.get(&task_id) {
            self.graph
                .neighbors_directed(node, Direction::Outgoing)
                .map(|n| self.graph[n])
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 检测是否存在循环依赖
    pub fn has_cycle(&self) -> bool {
        toposort(&self.graph, None).is_err()
    }

    /// 获取拓扑排序
    pub fn topological_sort(&self) -> Result<Vec<Uuid>> {
        match toposort(&self.graph, None) {
            Ok(nodes) => Ok(nodes.into_iter().map(|n| self.graph[n]).collect()),
            Err(_) => Err(QueueError::CircularDependency("依赖图存在循环".to_string())),
        }
    }

    /// 检查任务是否存在
    pub fn contains(&self, task_id: Uuid) -> bool {
        self.task_map.contains_key(&task_id)
    }

    /// 获取任务数量
    pub fn len(&self) -> usize {
        self.task_map.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.task_map.is_empty()
    }

    /// 清空依赖图
    pub fn clear(&mut self) {
        self.graph.clear();
        self.task_map.clear();
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_task(deps: Vec<Uuid>) -> Task {
        let mut task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );
        task.dependencies = deps;
        task
    }

    #[test]
    fn test_add_task() {
        let mut graph = DependencyGraph::new();

        let task1 = create_task(vec![]);
        let task2 = create_task(vec![task1.id]);
        let task3 = create_task(vec![task2.id]);

        graph.add_task(&task1).unwrap();
        graph.add_task(&task2).unwrap();
        graph.add_task(&task3).unwrap();

        assert_eq!(graph.len(), 3);
        assert!(graph.contains(task1.id));
        assert!(graph.contains(task2.id));
        assert!(graph.contains(task3.id));
    }

    #[test]
    fn test_get_ready_tasks() {
        let mut graph = DependencyGraph::new();

        let task1 = create_task(vec![]);
        let task2 = create_task(vec![]);
        let task3 = create_task(vec![task1.id, task2.id]);

        graph.add_task(&task1).unwrap();
        graph.add_task(&task2).unwrap();
        graph.add_task(&task3).unwrap();

        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&task1.id));
        assert!(ready.contains(&task2.id));
        assert!(!ready.contains(&task3.id));
    }

    #[test]
    fn test_get_dependencies() {
        let mut graph = DependencyGraph::new();

        let task1 = create_task(vec![]);
        let task2 = create_task(vec![]);
        let task3 = create_task(vec![task1.id, task2.id]);

        graph.add_task(&task1).unwrap();
        graph.add_task(&task2).unwrap();
        graph.add_task(&task3).unwrap();

        let deps = graph.get_dependencies(task3.id);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&task1.id));
        assert!(deps.contains(&task2.id));
    }

    #[test]
    fn test_get_dependents() {
        let mut graph = DependencyGraph::new();

        let task1 = create_task(vec![]);
        let task2 = create_task(vec![task1.id]);
        let task3 = create_task(vec![task1.id]);

        graph.add_task(&task1).unwrap();
        graph.add_task(&task2).unwrap();
        graph.add_task(&task3).unwrap();

        let dependents = graph.get_dependents(task1.id);
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&task2.id));
        assert!(dependents.contains(&task3.id));
    }

    #[test]
    fn test_circular_dependency() {
        let mut graph = DependencyGraph::new();

        let task1 = create_task(vec![]);
        let task1_id = task1.id;

        graph.add_task(&task1).unwrap();

        let mut task2 = create_task(vec![task1_id]);
        let task2_id = task2.id;
        graph.add_task(&task2).unwrap();

        // 创建循环依赖：task1 依赖 task2
        let mut task1_updated = create_task(vec![task2_id]);
        task1_updated.id = task1_id;

        // 移除旧的 task1，添加新的
        graph.remove_task(task1_id);
        let result = graph.add_task(&task1_updated);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            QueueError::CircularDependency(_)
        ));
    }

    #[test]
    fn test_remove_task() {
        let mut graph = DependencyGraph::new();

        let task1 = create_task(vec![]);
        let task2 = create_task(vec![task1.id]);

        graph.add_task(&task1).unwrap();
        graph.add_task(&task2).unwrap();

        assert_eq!(graph.len(), 2);

        graph.remove_task(task1.id);
        assert_eq!(graph.len(), 1);
        assert!(!graph.contains(task1.id));
        assert!(graph.contains(task2.id));
    }

    #[test]
    fn test_topological_sort() {
        let mut graph = DependencyGraph::new();

        let task1 = create_task(vec![]);
        let task2 = create_task(vec![task1.id]);
        let task3 = create_task(vec![task2.id]);

        graph.add_task(&task1).unwrap();
        graph.add_task(&task2).unwrap();
        graph.add_task(&task3).unwrap();

        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted.len(), 3);

        // 验证顺序
        let pos1 = sorted.iter().position(|&id| id == task1.id).unwrap();
        let pos2 = sorted.iter().position(|&id| id == task2.id).unwrap();
        let pos3 = sorted.iter().position(|&id| id == task3.id).unwrap();

        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
    }

    #[test]
    fn test_duplicate_task() {
        let mut graph = DependencyGraph::new();

        let task = create_task(vec![]);
        graph.add_task(&task).unwrap();

        let result = graph.add_task(&task);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), QueueError::DuplicateTask(_)));
    }

    #[test]
    fn test_clear() {
        let mut graph = DependencyGraph::new();

        let task1 = create_task(vec![]);
        let task2 = create_task(vec![task1.id]);

        graph.add_task(&task1).unwrap();
        graph.add_task(&task2).unwrap();

        assert_eq!(graph.len(), 2);

        graph.clear();
        assert!(graph.is_empty());
    }
}
