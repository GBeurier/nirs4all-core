//! Rust surface for the nirs4all-lite aggregate distribution.
//!
//! This crate intentionally starts as a registry and namespace layer. Runtime
//! functionality must delegate to the owning upstream crates.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upstream {
    pub key: &'static str,
    pub package: &'static str,
    pub role: &'static str,
}

pub const UPSTREAMS: &[Upstream] = &[
    Upstream {
        key: "dag_ml",
        package: "dag-ml",
        role: "Leakage-safe DAG/ML execution coordinator",
    },
    Upstream {
        key: "dag_ml_data",
        package: "dag-ml-data",
        role: "Sample-aligned data contracts for DAG/ML runtimes",
    },
    Upstream {
        key: "formats",
        package: "nirs4all-formats",
        role: "Spectroscopy/NIRS vendor file readers",
    },
    Upstream {
        key: "io",
        package: "nirs4all-io",
        role: "Dataset assembly bridge",
    },
    Upstream {
        key: "datasets",
        package: "nirs4all-datasets",
        role: "DOI-pinned NIRS dataset catalog",
    },
    Upstream {
        key: "methods",
        package: "nirs4all-methods",
        role: "Portable C ABI PLS/NIRS numerical engine",
    },
];

pub fn upstream(key: &str) -> Option<&'static Upstream> {
    UPSTREAMS.iter().find(|item| item.key == key)
}

pub mod dag_ml {
    pub const UPSTREAM_KEY: &str = "dag_ml";
}

pub mod dag_ml_data {
    pub const UPSTREAM_KEY: &str = "dag_ml_data";
}

pub mod datasets {
    pub const UPSTREAM_KEY: &str = "datasets";
}

pub mod formats {
    pub const UPSTREAM_KEY: &str = "formats";
}

pub mod io {
    pub const UPSTREAM_KEY: &str = "io";
}

pub mod methods {
    pub const UPSTREAM_KEY: &str = "methods";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_expected_upstream_keys() {
        let keys: Vec<_> = UPSTREAMS.iter().map(|item| item.key).collect();
        assert_eq!(
            keys,
            vec!["dag_ml", "dag_ml_data", "formats", "io", "datasets", "methods"]
        );
    }

    #[test]
    fn resolves_upstream_by_key() {
        assert_eq!(upstream("methods").unwrap().package, "nirs4all-methods");
        assert!(upstream("unknown").is_none());
    }
}
