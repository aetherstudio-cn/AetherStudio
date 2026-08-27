//! 行级差异算法（AI 面板差异可视化渲染用）
//!
//! 策略：先剥离新旧内容的首尾公共行（典型编辑只改动文件中部），
//! 对中间变化区做 LCS；中间区过大（行数乘积超上限）时退化为
//! 「旧区全删 + 新区全增」，保证渲染帧成本有上限。

/// 差异行类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    /// 未修改行
    Same,
    /// 新增行（新版内容）
    Add,
    /// 删除行（旧版内容）
    Del,
}

/// 单个差异行
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// LCS 中间区行数乘积上限（约 40 万单元格 = 1.6MB DP 表），超出退化为全删+全增
const LCS_CELL_CAP: usize = 400_000;

/// 计算行级差异：未修改行保持原文，新增/删除行分别标记。
pub fn line_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    // 公共前缀
    let mut prefix = 0;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && old_lines[prefix] == new_lines[prefix]
    {
        prefix += 1;
    }
    // 公共后缀（不与前缀重叠）
    let mut suffix = 0;
    while suffix < old_lines.len() - prefix
        && suffix < new_lines.len() - prefix
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let mid_old = &old_lines[prefix..old_lines.len() - suffix];
    let mid_new = &new_lines[prefix..new_lines.len() - suffix];

    let mut out: Vec<DiffLine> = Vec::with_capacity(old_lines.len() + new_lines.len());
    for l in &old_lines[..prefix] {
        out.push(DiffLine {
            kind: DiffKind::Same,
            text: l.to_string(),
        });
    }

    if mid_old.len() * mid_new.len() <= LCS_CELL_CAP {
        // LCS 动态规划（自底向上填表，回溯产出最短编辑序列）
        let (n, m) = (mid_old.len(), mid_new.len());
        let mut dp = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                dp[i][j] = if mid_old[i] == mid_new[j] {
                    dp[i + 1][j + 1] + 1
                } else {
                    dp[i + 1][j].max(dp[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if mid_old[i] == mid_new[j] {
                out.push(DiffLine {
                    kind: DiffKind::Same,
                    text: mid_old[i].to_string(),
                });
                i += 1;
                j += 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                out.push(DiffLine {
                    kind: DiffKind::Del,
                    text: mid_old[i].to_string(),
                });
                i += 1;
            } else {
                out.push(DiffLine {
                    kind: DiffKind::Add,
                    text: mid_new[j].to_string(),
                });
                j += 1;
            }
        }
        while i < n {
            out.push(DiffLine {
                kind: DiffKind::Del,
                text: mid_old[i].to_string(),
            });
            i += 1;
        }
        while j < m {
            out.push(DiffLine {
                kind: DiffKind::Add,
                text: mid_new[j].to_string(),
            });
            j += 1;
        }
    } else {
        // 超大变化区：退化为整段删+整段增，避免 DP 成本失控
        for l in mid_old {
            out.push(DiffLine {
                kind: DiffKind::Del,
                text: l.to_string(),
            });
        }
        for l in mid_new {
            out.push(DiffLine {
                kind: DiffKind::Add,
                text: l.to_string(),
            });
        }
    }

    for l in &old_lines[old_lines.len() - suffix..] {
        out.push(DiffLine {
            kind: DiffKind::Same,
            text: l.to_string(),
        });
    }
    out
}

/// 统计差异中的新增/删除行数，返回 `(added, removed)`。
pub fn diff_stats(diff: &[DiffLine]) -> (usize, usize) {
    let added = diff.iter().filter(|l| l.kind == DiffKind::Add).count();
    let removed = diff.iter().filter(|l| l.kind == DiffKind::Del).count();
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_all_same() {
        let diff = line_diff("a\nb\nc", "a\nb\nc");
        assert!(diff.iter().all(|l| l.kind == DiffKind::Same));
        assert_eq!(diff_stats(&diff), (0, 0));
    }

    #[test]
    fn middle_change_marked_add_del() {
        let diff = line_diff("a\nold1\nold2\nd", "a\nnew1\nd");
        // 前缀 a、后缀 d 保持 Same
        assert_eq!(diff[0].kind, DiffKind::Same);
        assert_eq!(diff.last().unwrap().kind, DiffKind::Same);
        let (added, removed) = diff_stats(&diff);
        assert_eq!(added, 1);
        assert_eq!(removed, 2);
    }

    #[test]
    fn pure_append_only_add() {
        let diff = line_diff("a\nb", "a\nb\nc\nd");
        assert_eq!(diff_stats(&diff), (2, 0));
    }

    #[test]
    fn pure_delete_only_del() {
        let diff = line_diff("a\nb\nc\nd", "a\nd");
        assert_eq!(diff_stats(&diff), (0, 2));
    }

    #[test]
    fn empty_old_all_add() {
        let diff = line_diff("", "x\ny");
        assert_eq!(diff_stats(&diff), (2, 0));
    }

    #[test]
    fn oversized_middle_falls_back() {
        // 构造超过 LCS_CELL_CAP 的中间区：700x700 = 49 万 > 40 万
        let old: Vec<String> = (0..700).map(|i| format!("o{}", i)).collect();
        let new: Vec<String> = (0..700).map(|i| format!("n{}", i)).collect();
        let diff = line_diff(&old.join("\n"), &new.join("\n"));
        assert_eq!(diff_stats(&diff), (700, 700));
    }

    #[test]
    fn lcs_keeps_common_middle_lines() {
        // 中间区含公共行时 LCS 应保留为 Same
        let diff = line_diff("keep\nx\ny", "keep\nx\nz\ny");
        let same_texts: Vec<&str> = diff
            .iter()
            .filter(|l| l.kind == DiffKind::Same)
            .map(|l| l.text.as_str())
            .collect();
        assert!(same_texts.contains(&"x"));
        assert!(same_texts.contains(&"y"));
        assert_eq!(diff_stats(&diff), (1, 0));
    }
}
