//! HNSW 近似最近邻索引（纯 Rust，内存常驻）
//!
//! - 余弦距离（向量入库前 L2 归一化，距离 = 1 - 内积）
//! - 增量插入、墓碑删除（搜索时跳过；compaction 后由上层重建）
//! - 参数量身定制编辑器规模：M=8、Mmax0=16、ef_construction=64

/// 距离度量：归一化向量的余弦距离（0 完全相同，2 完全相反）
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len().min(b.len()) {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= 0.0 || nb <= 0.0 {
        return 2.0;
    }
    1.0 - dot / (na.sqrt() * nb.sqrt())
}

/// L2 归一化（零向量原样返回）
pub fn normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= 0.0 {
        return v.to_vec();
    }
    v.iter().map(|x| x / norm).collect()
}

const M: usize = 8; // 每层最大出边数
const M_MAX0: usize = 16; // 第 0 层最大出边数
const EF_CONSTRUCTION: usize = 64;
const MAX_LEVEL_CAP: usize = 8;

pub struct Hnsw {
    #[allow(dead_code)]
    dim: usize,
    /// 归一化向量（按节点 id）
    pub(crate) vectors: Vec<Vec<f32>>,
    /// 外部 key（上层记录的稳定标识）
    pub(crate) keys: Vec<u64>,
    /// links[node][level] = 邻居节点 id 列表
    links: Vec<Vec<Vec<u32>>>,
    /// 墓碑标记
    pub(crate) deleted: Vec<bool>,
    entry: Option<usize>,
    max_level: usize,
    /// xorshift64 状态（确定性弱随机即可，HNSW 对随机源不敏感）
    rng: u64,
}

impl Hnsw {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            vectors: Vec::new(),
            keys: Vec::new(),
            links: Vec::new(),
            deleted: Vec::new(),
            entry: None,
            max_level: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// 存活（未删除）节点数
    #[allow(dead_code)]
    pub fn live_count(&self) -> usize {
        self.deleted.iter().filter(|d| !**d).count()
    }

    #[allow(dead_code)]
    pub fn dim(&self) -> usize {
        self.dim
    }

    fn rand_u64(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    /// 随机层级：floor(-ln(U) * mL)，mL = 1/ln(M)
    fn random_level(&mut self) -> usize {
        let ml = 1.0 / (M as f64).ln();
        let u = ((self.rand_u64() >> 11) as f64 + 1.0) / (1u64 << 53) as f64;
        let lvl = (-u.ln() * ml).floor() as usize;
        lvl.min(MAX_LEVEL_CAP)
    }

    /// 单层 beam search：从 entry_points 出发返回至多 ef 个最近节点（含已删除，由调用方过滤）
    fn search_layer(
        &self,
        q: &[f32],
        entry_points: &[usize],
        ef: usize,
        level: usize,
    ) -> Vec<(usize, f32)> {
        let mut visited = vec![false; self.vectors.len()];
        // candidates：待扩展（按距离升序取头）；results：当前最优 ef 个
        let mut candidates: Vec<(usize, f32)> = Vec::new();
        let mut results: Vec<(usize, f32)> = Vec::new();
        for &ep in entry_points {
            if ep < self.vectors.len() && !visited[ep] {
                visited[ep] = true;
                let d = cosine_distance(q, &self.vectors[ep]);
                candidates.push((ep, d));
                results.push((ep, d));
            }
        }
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        while let Some(&(c, d_c)) = candidates.first() {
            // 候选比当前 ef 边界还远：收敛终止
            if results.len() >= ef && d_c > results[ef - 1].1 {
                break;
            }
            candidates.remove(0);
            let neighbors = self
                .links
                .get(c)
                .and_then(|l| l.get(level))
                .cloned()
                .unwrap_or_default();
            for n in neighbors {
                let n = n as usize;
                if n >= self.vectors.len() || visited[n] {
                    continue;
                }
                visited[n] = true;
                let d_n = cosine_distance(q, &self.vectors[n]);
                let worst = results.last().map(|x| x.1);
                if results.len() < ef || worst.map(|w| d_n < w).unwrap_or(true) {
                    candidates.push((n, d_n));
                    results.push((n, d_n));
                    results
                        .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                    if results.len() > ef {
                        results.pop();
                    }
                    candidates
                        .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                }
            }
        }
        results
    }

    /// 裁剪邻居列表到上限（保留最近的）
    fn prune_links(&mut self, node: usize, level: usize, max: usize) {
        let mut nb = self.links[node][level].clone();
        if nb.len() <= max {
            return;
        }
        let v = &self.vectors[node];
        let mut with_d: Vec<(u32, f32)> = nb
            .iter()
            .map(|&n| (n, cosine_distance(v, &self.vectors[n as usize])))
            .collect();
        with_d.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        nb = with_d.into_iter().take(max).map(|(n, _)| n).collect();
        self.links[node][level] = nb;
    }

    /// 插入向量；返回节点 id。ext_key 为上层记录标识（同 key 重插由上层先删旧节点）
    pub fn insert(&mut self, ext_key: u64, vector: &[f32]) -> usize {
        let v = normalize(vector);
        let level = self.random_level();
        let id = self.vectors.len();
        self.vectors.push(v.clone());
        self.keys.push(ext_key);
        self.deleted.push(false);
        self.links.push(vec![Vec::new(); level + 1]);

        let Some(mut ep) = self.entry else {
            self.entry = Some(id);
            self.max_level = level;
            return id;
        };

        // 高层（> level）：单点贪心下降
        for lc in (level + 1..=self.max_level).rev() {
            let res = self.search_layer(&v, &[ep], 1, lc);
            if let Some(&(n, _)) = res.first() {
                ep = n;
            }
        }
        // 0..=min(level, max_level)：beam search 建边
        for lc in (0..=level.min(self.max_level)).rev() {
            let neighbors = self.search_layer(&v, &[ep], EF_CONSTRUCTION, lc);
            let max_conn = if lc == 0 { M_MAX0 } else { M };
            let selected: Vec<usize> = neighbors.iter().take(M).map(|&(n, _)| n).collect();
            for &n in &selected {
                self.links[id][lc].push(n as u32);
                if n < self.links.len() && lc < self.links[n].len() {
                    self.links[n][lc].push(id as u32);
                    self.prune_links(n, lc, max_conn);
                }
            }
            if let Some(&(closest, _)) = neighbors.first() {
                ep = closest;
            }
        }
        if level > self.max_level {
            self.entry = Some(id);
            self.max_level = level;
        }
        id
    }

    /// 墓碑删除（保留图结构，搜索时跳过）
    pub fn mark_deleted(&mut self, node: usize) {
        if node < self.deleted.len() {
            self.deleted[node] = true;
        }
    }

    /// KNN 搜索：返回 (ext_key, distance)，按距离升序，跳过墓碑
    pub fn search(&self, query: &[f32], k: usize, ef: usize) -> Vec<(u64, f32)> {
        if self.vectors.is_empty() || k == 0 {
            return Vec::new();
        }
        let q = normalize(query);
        // 小规模直接暴力精确搜索（结果更准，开销可忽略）
        if self.vectors.len() <= 2048 {
            let mut all: Vec<(u64, f32)> = self
                .vectors
                .iter()
                .enumerate()
                .filter(|(i, _)| !self.deleted[*i])
                .map(|(i, v)| (self.keys[i], cosine_distance(&q, v)))
                .collect();
            all.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            all.truncate(k);
            return all;
        }
        let mut ep = match self.entry {
            Some(e) => e,
            None => return Vec::new(),
        };
        for lc in (1..=self.max_level).rev() {
            let res = self.search_layer(&q, &[ep], 1, lc);
            if let Some(&(n, _)) = res.first() {
                ep = n;
            }
        }
        let ef = ef.max(k);
        let res = self.search_layer(&q, &[ep], ef, 0);
        res.into_iter()
            .filter(|(n, _)| !self.deleted[*n])
            .take(k)
            .map(|(n, d)| (self.keys[n], d))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_search_basic() {
        let mut h = Hnsw::new(4);
        h.insert(1, &[1.0, 0.0, 0.0, 0.0]);
        h.insert(2, &[0.0, 1.0, 0.0, 0.0]);
        h.insert(3, &[0.9, 0.1, 0.0, 0.0]);

        let res = h.search(&[1.0, 0.0, 0.0, 0.0], 2, 16);
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].0, 1, "完全相同向量应排第一");
        assert_eq!(res[1].0, 3, "次近应为 [0.9,0.1]");
    }

    #[test]
    fn test_tombstone_skipped() {
        let mut h = Hnsw::new(3);
        let n0 = h.insert(10, &[1.0, 0.0, 0.0]);
        h.insert(11, &[0.95, 0.05, 0.0]);
        h.mark_deleted(n0);
        let res = h.search(&[1.0, 0.0, 0.0], 5, 16);
        assert!(res.iter().all(|(k, _)| *k != 10), "墓碑节点不应返回");
        assert_eq!(h.live_count(), 1);
    }

    #[test]
    fn test_large_random_recall() {
        // 2500 个随机向量（>2048 走 HNSW 路径），验证 top-1 召回与暴力一致
        let dim = 16;
        let n = 2500;
        let mut h = Hnsw::new(dim);
        let mut rng = 0x1234_5678u64;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 40) as f32 / (1u64 << 24) as f32
        };
        let mut vecs = Vec::new();
        for i in 0..n {
            let v: Vec<f32> = (0..dim).map(|_| next() - 0.5).collect();
            vecs.push(v.clone());
            h.insert(i as u64, &v);
        }
        let mut hits = 0;
        for trial in 0..20 {
            let q: Vec<f32> = vecs[(trial * 97) % n].clone();
            // 暴力真值
            let mut brute: Vec<(u64, f32)> = vecs
                .iter()
                .enumerate()
                .map(|(i, v)| (i as u64, cosine_distance(&q, v)))
                .collect();
            brute.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            let expect_top = brute[0].0;
            let res = h.search(&q, 1, 64);
            if !res.is_empty() && res[0].0 == expect_top {
                hits += 1;
            }
        }
        assert!(hits >= 18, "HNSW top-1 召回率过低: {}/20", hits);
    }

    #[test]
    fn test_normalize_and_distance() {
        let v = normalize(&[3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((cosine_distance(&[1.0, 0.0], &[1.0, 0.0])).abs() < 1e-6);
        assert!((cosine_distance(&[1.0, 0.0], &[0.0, 1.0]) - 1.0).abs() < 1e-6);
    }
}
