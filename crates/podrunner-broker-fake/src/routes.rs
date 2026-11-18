//! The routes that this broker does not serve, with the entry of each.

/// The service of a route that this broker does not serve, and the entry
/// that gives it, when it is known (T-145).
pub fn service_of(path: &str) -> Option<&'static str> {
    let known: [(&str, &str); 5] = [
        ("ArtifactService", "the ArtifactService of the results API (T-119)"),
        ("CacheService", "the CacheService of the results API (T-120)"),
        (
            "_apis/artifactcache",
            "the legacy cache routes, which GitHub turned off; actions/cache@v4 takes the CacheService (T-120)",
        ),
        ("_apis/pipelines/workflows", "the legacy artifact routes of v3, which GitHub turned off (T-119)"),
        ("idtoken", "the OIDC id-token of the job (T-140)"),
    ];
    known.iter().find(|(k, _)| path.contains(k)).map(|(_, s)| *s)
}
