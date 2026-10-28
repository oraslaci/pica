use super::traits::FaceClusterer;

/// Picasa Sow-Grow face clusterer with cosine similarity metric.
pub struct SowGrowFaceClusterer;

impl Default for SowGrowFaceClusterer {
    fn default() -> Self {
        Self::new()
    }
}

impl SowGrowFaceClusterer {
    pub fn new() -> Self {
        Self
    }

    /// Computes cosine similarity between two normalized feature vectors
    #[inline]
    pub fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
        picasa_face::cosine_similarity(v1, v2)
    }
}

impl FaceClusterer for SowGrowFaceClusterer {
    fn find_matching_cluster(
        &self,
        embedding: &[f32],
        existing_clusters: &[(i64, Vec<f32>)],
        threshold: f32,
    ) -> Option<i64> {
        let clusterer = picasa_face::SowGrowClusterer::new(picasa_face::SowGrowConfig {
            grow_threshold: threshold,
            ..picasa_face::SowGrowConfig::default()
        });
        clusterer.find_matching_cluster(embedding, existing_clusters)
    }
}

pub type CosineFaceClusterer = SowGrowFaceClusterer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_and_clustering() {
        let clusterer = SowGrowFaceClusterer::new();

        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![0.95, 0.05, 0.0];
        let v3 = vec![0.0, 1.0, 0.0];

        let sim12 = SowGrowFaceClusterer::cosine_similarity(&v1, &v2);
        let sim13 = SowGrowFaceClusterer::cosine_similarity(&v1, &v3);

        assert!(sim12 > 0.9);
        assert!(sim13 < 0.1);

        let clusters = vec![(101, v1), (102, v3)];

        // Matching query vector
        let matched = clusterer.find_matching_cluster(&v2, &clusters, 0.70);
        assert_eq!(matched, Some(101));

        // Unmatched query vector
        let v4 = vec![0.0, 0.0, 1.0];
        let unmatched = clusterer.find_matching_cluster(&v4, &clusters, 0.70);
        assert_eq!(unmatched, None);
    }

    #[test]
    fn test_cannot_link_and_matching() {
        let clusterer = SowGrowFaceClusterer::new();

        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![0.98, 0.02, 0.0];
        let clusters = vec![(101, v1)];

        // Matches when above threshold
        assert_eq!(clusterer.find_matching_cluster(&v2, &clusters, 0.48), Some(101));

        // Does not match when below threshold
        let v3 = vec![0.0, 1.0, 0.0];
        assert_eq!(clusterer.find_matching_cluster(&v3, &clusters, 0.48), None);
    }
}
