use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

/// High-performance Disjoint-Set (Union-Find) with path compression and union-by-rank.
/// Directly mirrors the algorithm found at address 0x0081b4b0 in Picasa3.exe.
#[derive(Debug, Clone)]
pub struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl DisjointSet {
    pub fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    /// Find root with full path compression
    pub fn find(&mut self, mut x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        // Path compression
        while self.parent[x] != root {
            let next = self.parent[x];
            self.parent[x] = root;
            x = next;
        }
        root
    }

    /// Union by rank
    pub fn union(&mut self, x: usize, y: usize) -> usize {
        let root_x = self.find(x);
        let root_y = self.find(y);

        if root_x == root_y {
            return root_x;
        }

        if self.rank[root_x] < self.rank[root_y] {
            self.parent[root_x] = root_y;
            root_y
        } else if self.rank[root_x] > self.rank[root_y] {
            self.parent[root_y] = root_x;
            root_x
        } else {
            self.parent[root_y] = root_x;
            self.rank[root_x] += 1;
            root_x
        }
    }
}

/// A face record input to the clustering engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceClusterItem {
    pub face_id: i64,
    pub photo_id: i64,
    pub embedding: Vec<f32>,
}

/// Resulting cluster representing a Person entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonCluster {
    pub root_face_id: i64,
    pub face_ids: Vec<i64>,
    pub centroid_embedding: Vec<f32>,
    #[serde(default)]
    pub photo_ids: Vec<i64>,
    #[serde(default)]
    pub exemplar_embeddings: Vec<Vec<f32>>,
}

/// Configuration parameters matching Picasa's exact binary values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SowGrowConfig {
    /// Similarity threshold to establish a seed pair (Sow phase).
    /// Default: 0.78 (Picasa's FRSuggestionSingleThreshold = 0x5a = 90 in UI, internal seed = 0.78)
    pub sow_threshold: f32,

    /// Similarity threshold to expand a seed cluster with a candidate (Grow phase).
    /// Default: 0.73 (Picasa's internal Grow threshold in Red.dll)
    pub grow_threshold: f32,

    /// Similarity threshold to rank suggestions in the user UI.
    /// Default: 0.50 (Picasa's FRSortThreshold = 0x32 = 50)
    pub sort_threshold: f32,

    /// Maximum number of suggestion candidates to return.
    /// Default: 50 (Picasa's FRRecommendIncludeLimit = 50)
    pub max_recommendations: usize,

    /// Enforce photo-level Cannot-Link invariant (faces in same photo cannot merge).
    /// Default: true
    pub cannot_link: bool,

    /// Weight of exemplar stamp matching vs centroid matching (0.0 to 1.0).
    /// Default: 0.70 (70% exemplar, 30% centroid)
    pub exemplar_weight: f32,
}

impl Default for SowGrowConfig {
    fn default() -> Self {
        Self {
            sow_threshold: 0.85,
            grow_threshold: 0.80,
            sort_threshold: 0.50,
            max_recommendations: 50,
            cannot_link: true,
            exemplar_weight: 0.70,
        }
    }
}

/// Result of running Picasa Sow-Grow clustering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SowGrowResult {
    /// Final clusters (each representing a person with >= 1 faces)
    pub clusters: Vec<PersonCluster>,
    /// Unassigned faces that did not meet the grow threshold with any cluster
    pub unassigned_face_ids: Vec<i64>,
}

/// Computes cosine similarity between vectors: dot / (norm1 * norm2)
#[inline]
pub fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    if v1.len() != v2.len() || v1.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut n1 = 0.0f32;
    let mut n2 = 0.0f32;
    for (a, b) in v1.iter().zip(v2.iter()) {
        dot += a * b;
        n1 += a * a;
        n2 += b * b;
    }
    let denom = (n1 * n2).sqrt();
    if denom > 1e-6 {
        (dot / denom).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Authentic Picasa Sow-Grow Clusterer (`vfr_SowGrowClusterer` & `vfr_SowGrowStampClusterer`)
#[derive(Debug, Clone)]
pub struct SowGrowClusterer {
    pub config: SowGrowConfig,
}

impl Default for SowGrowClusterer {
    fn default() -> Self {
        Self::new(SowGrowConfig::default())
    }
}

impl SowGrowClusterer {
    pub fn new(config: SowGrowConfig) -> Self {
        Self { config }
    }

    /// Run full two-phase Sow-Grow clustering on a batch of face items
    pub fn cluster(&self, items: &[FaceClusterItem]) -> SowGrowResult {
        let n = items.len();
        if n == 0 {
            return SowGrowResult {
                clusters: Vec::new(),
                unassigned_face_ids: Vec::new(),
            };
        }
        if n == 1 {
            return SowGrowResult {
                clusters: vec![PersonCluster {
                    root_face_id: items[0].face_id,
                    face_ids: vec![items[0].face_id],
                    centroid_embedding: items[0].embedding.clone(),
                    photo_ids: vec![items[0].photo_id],
                    exemplar_embeddings: vec![items[0].embedding.clone()],
                }],
                unassigned_face_ids: Vec::new(),
            };
        }

        // ==========================================
        // PHASE 1: SOW (High-confidence Seed Search)
        // Disassembled from Red.dll VA 0x100a61d0
        // ==========================================
        let mut uf = DisjointSet::new(n);

        // Pre-filter Cannot-Link sibling faces in same photo
        let mut candidate_edges = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if self.config.cannot_link && items[i].photo_id == items[j].photo_id {
                    continue; // Sibling Cannot-Link invariant
                }
                let sim = cosine_similarity(&items[i].embedding, &items[j].embedding);
                if sim >= self.config.sow_threshold {
                    candidate_edges.push((i, j, sim));
                }
            }
        }

        // Sort descending by similarity
        candidate_edges.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Track member indices and photo set per component root
        let mut comp_members: HashMap<usize, Vec<usize>> = (0..n).map(|i| (i, vec![i])).collect();
        let mut comp_photos: HashMap<usize, HashSet<i64>> = (0..n)
            .map(|i| {
                let mut s = HashSet::new();
                s.insert(items[i].photo_id);
                (i, s)
            })
            .collect();

        // Perform union on high-confidence seed pairs respecting Cannot-Link
        for (i, j, _sim) in candidate_edges {
            let root_i = uf.find(i);
            let root_j = uf.find(j);

            if root_i == root_j {
                continue;
            }

            if self.config.cannot_link {
                let photos_i = &comp_photos[&root_i];
                let photos_j = &comp_photos[&root_j];

                let has_photo_conflict = if photos_i.len() < photos_j.len() {
                    photos_i.iter().any(|pid| photos_j.contains(pid))
                } else {
                    photos_j.iter().any(|pid| photos_i.contains(pid))
                };

                if has_photo_conflict {
                    continue; // Cannot-Link violation
                }
            }

            let new_root = uf.union(root_i, root_j);
            let other_root = if new_root == root_i { root_j } else { root_i };

            let other_mems = comp_members.remove(&other_root).unwrap_or_default();
            let other_p = comp_photos.remove(&other_root).unwrap_or_default();

            comp_members.get_mut(&new_root).unwrap().extend(other_mems);
            comp_photos.get_mut(&new_root).unwrap().extend(other_p);
        }

        // Internal working cluster structure for Phase 2
        // Implements exemplar stamping as seen in vfr_SowGrowStampClusterer
        struct ActiveCluster {
            members: Vec<usize>,
            exemplar_indices: Vec<usize>, // Seed exemplars / stamps
            photos: HashSet<i64>,
            centroid: Vec<f32>,
        }

        let dim = items[0].embedding.len();
        let compute_centroid = |member_indices: &[usize]| -> Vec<f32> {
            let mut c = vec![0.0f32; dim];
            for &idx in member_indices {
                for (cv, ev) in c.iter_mut().zip(items[idx].embedding.iter()) {
                    *cv += *ev;
                }
            }
            let norm: f32 = c.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
            for cv in &mut c {
                *cv /= norm;
            }
            c
        };

        let mut clusters: Vec<ActiveCluster> = Vec::new();
        let mut assigned_mask = vec![false; n];

        // Seed clusters are those with at least 2 faces meeting sow_threshold
        for (_root, members) in comp_members {
            if members.len() >= 2 {
                for &m in &members {
                    assigned_mask[m] = true;
                }
                let mut photos = HashSet::new();
                for &m in &members {
                    photos.insert(items[m].photo_id);
                }
                let centroid = compute_centroid(&members);
                // The initial members form the exemplar stamps (up to 5)
                let exemplar_indices = members.iter().take(5).copied().collect();
                clusters.push(ActiveCluster {
                    members,
                    exemplar_indices,
                    photos,
                    centroid,
                });
            }
        }

        // ==========================================
        // PHASE 2: GROW (Exemplar-Stamped Prim-style Spanning Expansion)
        // Disassembled from Red.dll VA 0x100a66c0 & 0x100a0c38
        // ==========================================
        let mut unassigned_indices: Vec<usize> = (0..n).filter(|&i| !assigned_mask[i]).collect();
        let ew = self.config.exemplar_weight.clamp(0.0, 1.0);
        let cw = 1.0 - ew;

        // Iteratively expand clusters with candidate faces that exceed grow_threshold
        loop {
            let mut best_candidate: Option<(usize, usize, f32)> = None; // (unassigned_idx, cluster_idx, sim)

            for (u_pos, &u_idx) in unassigned_indices.iter().enumerate() {
                let u_photo = items[u_idx].photo_id;

                for (c_idx, cluster) in clusters.iter().enumerate() {
                    if self.config.cannot_link && cluster.photos.contains(&u_photo) {
                        continue; // Cannot-Link invariant
                    }

                    // 1. Exemplar stamp similarity: max similarity across cluster exemplars
                    let max_exemplar_sim = cluster
                        .exemplar_indices
                        .iter()
                        .map(|&ex_idx| cosine_similarity(&items[u_idx].embedding, &items[ex_idx].embedding))
                        .fold(0.0f32, f32::max);

                    // 2. Centroid similarity
                    let centroid_sim = cosine_similarity(&items[u_idx].embedding, &cluster.centroid);

                    // 3. Combined score (anchored to closest exemplar while respecting cluster mass)
                    let sim = ew * max_exemplar_sim + cw * centroid_sim;

                    if sim >= self.config.grow_threshold {
                        match best_candidate {
                            Some((_, _, best_sim)) if sim > best_sim => {
                                best_candidate = Some((u_pos, c_idx, sim));
                            }
                            None => {
                                best_candidate = Some((u_pos, c_idx, sim));
                            }
                            _ => {}
                        }
                    }
                }
            }

            if let Some((best_u_pos, best_c_idx, best_sim)) = best_candidate {
                let u_idx = unassigned_indices.swap_remove(best_u_pos);
                let cluster = &mut clusters[best_c_idx];
                cluster.members.push(u_idx);
                cluster.photos.insert(items[u_idx].photo_id);
                cluster.centroid = compute_centroid(&cluster.members);

                // If candidate was an exceptionally strong match and cluster has < 5 exemplars, retain as exemplar stamp
                if best_sim >= self.config.sow_threshold && cluster.exemplar_indices.len() < 5 {
                    cluster.exemplar_indices.push(u_idx);
                }
            } else {
                // No remaining candidate meets grow_threshold
                break;
            }
        }

        // Convert unassigned remaining indices to face IDs
        let unassigned_face_ids: Vec<i64> = unassigned_indices.iter().map(|&idx| items[idx].face_id).collect();

        // Assemble PersonClusters
        let mut final_clusters: Vec<PersonCluster> = clusters
            .into_iter()
            .map(|c| {
                let face_ids: Vec<i64> = c.members.iter().map(|&idx| items[idx].face_id).collect();
                let photo_ids: Vec<i64> = c.photos.into_iter().collect();
                let exemplar_embeddings: Vec<Vec<f32>> = c
                    .exemplar_indices
                    .iter()
                    .map(|&idx| items[idx].embedding.clone())
                    .collect();
                PersonCluster {
                    root_face_id: face_ids[0],
                    face_ids,
                    centroid_embedding: c.centroid,
                    photo_ids,
                    exemplar_embeddings,
                }
            })
            .collect();

        // Picasa convention: sort descending by cluster member count
        final_clusters.sort_by(|a, b| b.face_ids.len().cmp(&a.face_ids.len()));

        SowGrowResult {
            clusters: final_clusters,
            unassigned_face_ids,
        }
    }

    /// Match a single face embedding against existing person clusters using grow_threshold
    pub fn find_matching_cluster(
        &self,
        query_embedding: &[f32],
        existing_centroids: &[(i64, Vec<f32>)], // (person_id, centroid)
    ) -> Option<i64> {
        let mut best: Option<(i64, f32)> = None;
        for (pid, centroid) in existing_centroids {
            let sim = cosine_similarity(query_embedding, centroid);
            if sim >= self.config.grow_threshold {
                match best {
                    Some((_, best_sim)) if sim > best_sim => {
                        best = Some((*pid, sim));
                    }
                    None => {
                        best = Some((*pid, sim));
                    }
                    _ => {}
                }
            }
        }
        best.map(|(pid, _)| pid)
    }

    /// Suggest candidate people for an unassigned face (UI suggestion tray)
    /// Disassembled from Picasa3.exe VA 0x48aab0 (FRSortThreshold = 0.50, limit = 50)
    pub fn suggest_candidates(
        &self,
        query_embedding: &[f32],
        existing_clusters: &[(i64, Vec<f32>, &[i64])], // (person_id, centroid, photo_ids)
        query_photo_id: Option<i64>,
    ) -> Vec<(i64, f32)> {
        let mut candidates = Vec::new();

        for (pid, centroid, photos) in existing_clusters {
            if self.config.cannot_link {
                if let Some(qid) = query_photo_id {
                    if photos.contains(&qid) {
                        continue; // Cannot-Link violation
                    }
                }
            }

            let sim = cosine_similarity(query_embedding, centroid);
            if sim >= self.config.sort_threshold {
                candidates.push((*pid, sim));
            }
        }

        // Sort descending by similarity
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(self.config.max_recommendations);
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disjoint_set_union_and_find() {
        let mut uf = DisjointSet::new(5);
        assert_eq!(uf.find(0), 0);
        assert_eq!(uf.find(1), 1);

        uf.union(0, 1);
        assert_eq!(uf.find(0), uf.find(1));

        uf.union(2, 3);
        assert_eq!(uf.find(2), uf.find(3));
        assert_ne!(uf.find(0), uf.find(2));

        uf.union(1, 3);
        assert_eq!(uf.find(0), uf.find(2));
    }

    #[test]
    fn test_cannot_link_same_photo() {
        let items = vec![
            FaceClusterItem { face_id: 1, photo_id: 100, embedding: vec![1.0, 0.0] },
            FaceClusterItem { face_id: 2, photo_id: 100, embedding: vec![1.0, 0.0] }, // Exact same vector, but same photo!
            FaceClusterItem { face_id: 3, photo_id: 200, embedding: vec![1.0, 0.0] }, // Same vector, different photo
        ];

        let clusterer = SowGrowClusterer::default();
        let res = clusterer.cluster(&items);

        // Face 1 and Face 3 should merge into one cluster. Face 2 must NOT be in that cluster!
        assert_eq!(res.clusters.len(), 1);
        assert_eq!(res.clusters[0].face_ids.len(), 2);
        assert!(res.clusters[0].face_ids.contains(&1));
        assert!(res.clusters[0].face_ids.contains(&3));
        assert_eq!(res.unassigned_face_ids, vec![2]);
    }

    #[test]
    fn test_sow_grow_two_distinct_people() {
        // Person A has 3 photos, Person B has 3 photos, Noise face has 1 photo
        let items = vec![
            // Person A (approx [1.0, 0.0])
            FaceClusterItem { face_id: 10, photo_id: 1, embedding: vec![0.98, 0.05] },
            FaceClusterItem { face_id: 11, photo_id: 2, embedding: vec![0.99, 0.02] },
            FaceClusterItem { face_id: 12, photo_id: 3, embedding: vec![0.95, 0.10] }, // Meets grow threshold (0.85)

            // Person B (approx [0.0, 1.0])
            FaceClusterItem { face_id: 20, photo_id: 4, embedding: vec![0.02, 0.99] },
            FaceClusterItem { face_id: 21, photo_id: 5, embedding: vec![0.05, 0.98] },
            FaceClusterItem { face_id: 22, photo_id: 6, embedding: vec![0.10, 0.95] },

            // Random ambiguous face [0.70, 0.70] (similarity ~0.70 to both, below 0.85)
            FaceClusterItem { face_id: 99, photo_id: 7, embedding: vec![0.707, 0.707] },
        ];

        let clusterer = SowGrowClusterer::default();
        let res = clusterer.cluster(&items);

        // Expect exactly 2 clusters: Person A (3 faces) and Person B (3 faces)
        assert_eq!(res.clusters.len(), 2);
        assert_eq!(res.clusters[0].face_ids.len(), 3);
        assert_eq!(res.clusters[1].face_ids.len(), 3);

        // Ambiguous face 99 must remain unassigned!
        assert_eq!(res.unassigned_face_ids, vec![99]);
    }

    #[test]
    fn test_suggest_candidates_sort_threshold() {
        let clusterer = SowGrowClusterer::default();
        let clusters = vec![
            (1001, vec![1.0, 0.0], [1, 2].as_slice()),
            (1002, vec![0.707, 0.707], [3].as_slice()),
            (1003, vec![0.0, 1.0], [4].as_slice()),
        ];

        let query = vec![0.90, 0.10]; // sim to 1001 ~0.99, sim to 1002 ~0.71, sim to 1003 ~0.11

        let suggestions = clusterer.suggest_candidates(&query, &clusters, Some(999));
        // Cluster 1001 and 1002 exceed 0.50, cluster 1003 does not
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].0, 1001);
        assert_eq!(suggestions[1].0, 1002);
    }
}
