use crate::error::GenreError;
use crate::resolver::{
    emit_content_resolve_span, ContributionKind, MergeStep, Provenance, Resolved, Tier,
};
use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Trait implemented by every struct with `#[derive(Layered)]`.
/// Allows the resolver to walk per-field merges across the four-tier chain.
pub trait LayeredMerge {
    /// Merge `other` (deeper tier) into `self` (shallower tier), producing the combined value.
    fn merge(self, other: Self) -> Self;
}

/// Loads tier files and applies the Layered merge walk, recording provenance.
pub struct Resolver<T> {
    root: PathBuf,
    _t: PhantomData<T>,
}

impl<T: DeserializeOwned> Resolver<T> {
    /// Create a new resolver rooted at `root`.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            _t: PhantomData,
        }
    }

    /// Load the World-tier file for `axis` under the path
    /// `{root}/{genre}/worlds/{world}/{axis}.yaml`.
    ///
    /// For Phase 1 this is a single-file World-tier load. Use
    /// [`Resolver::resolve_merged`] for the full Global → Genre → World → Culture walk.
    pub fn resolve(
        &self,
        axis: &str,
        ctx: &crate::resolver::ResolutionContext,
    ) -> Result<Resolved<T>, GenreError> {
        let world = ctx
            .world
            .as_ref()
            .ok_or_else(|| GenreError::ValidationError {
                message: "world is required for this axis".into(),
            })?;
        let path = self
            .root
            .join(&ctx.genre)
            .join("worlds")
            .join(world)
            .join(format!("{axis}.yaml"));
        let bytes = std::fs::read_to_string(&path).map_err(|e| GenreError::IoError {
            message: format!("reading {}: {}", path.display(), e),
        })?;
        let value: T = serde_yaml::from_str(&bytes).map_err(|e| GenreError::ValidationError {
            message: format!("parsing {}: {}", path.display(), e),
        })?;
        Ok(Resolved {
            value,
            provenance: Provenance {
                source_tier: Tier::World,
                source_file: path.clone(),
                source_span: None,
                merge_trail: vec![MergeStep {
                    tier: Tier::World,
                    file: path,
                    span: None,
                    contribution: ContributionKind::Initial,
                }],
            },
        })
    }
}

impl<T: DeserializeOwned + Default + Clone> Resolver<T> {
    /// Resolve a field path across Global → Genre → World → Culture, merging at each tier.
    ///
    /// `axis` is the semantic axis name (e.g. `"archetype"`, `"audio"`, `"image"`) used
    /// for the emitted `content.resolve` OTEL span's `content.axis` attribute and — in
    /// future phases — for axis-keyed observability and cache partitioning. It is
    /// decoupled from the on-disk `field_path` so the observability concept survives
    /// changes to the file layout.
    ///
    /// Requires that T implements the Layered `merge` method (via derive).
    pub fn resolve_merged(
        &self,
        axis: &str,
        field_path: &str,
        ctx: &crate::resolver::ResolutionContext,
    ) -> Result<Resolved<T>, GenreError>
    where
        T: LayeredMerge,
    {
        let start = Instant::now();
        let mut trail = Vec::new();
        let mut current: Option<T> = None;
        let mut final_tier = Tier::Global;
        let mut final_file = PathBuf::new();

        // Global tier
        let global_path = self.root.join(format!("{field_path}.yaml"));
        if let Ok(bytes) = std::fs::read_to_string(&global_path) {
            let val: T = serde_yaml::from_str(&bytes).map_err(|e| GenreError::ValidationError {
                message: format!("parsing {}: {}", global_path.display(), e),
            })?;
            current = Some(val);
            final_tier = Tier::Global;
            final_file = global_path.clone();
            trail.push(MergeStep {
                tier: Tier::Global,
                file: global_path,
                span: None,
                contribution: ContributionKind::Initial,
            });
        }

        // Genre tier
        let genre_path = self
            .root
            .join(&ctx.genre)
            .join(format!("{field_path}.yaml"));
        if let Ok(bytes) = std::fs::read_to_string(&genre_path) {
            let val: T = serde_yaml::from_str(&bytes).map_err(|e| GenreError::ValidationError {
                message: format!("parsing {}: {}", genre_path.display(), e),
            })?;
            let contribution = if current.is_some() {
                ContributionKind::Merged
            } else {
                ContributionKind::Initial
            };
            current = Some(match current {
                Some(base) => base.merge(val),
                None => val,
            });
            final_tier = Tier::Genre;
            final_file = genre_path.clone();
            trail.push(MergeStep {
                tier: Tier::Genre,
                file: genre_path,
                span: None,
                contribution,
            });
        }

        // World tier
        if let Some(world) = &ctx.world {
            let world_path = self
                .root
                .join(&ctx.genre)
                .join("worlds")
                .join(world)
                .join(format!("{field_path}.yaml"));
            if let Ok(bytes) = std::fs::read_to_string(&world_path) {
                let val: T =
                    serde_yaml::from_str(&bytes).map_err(|e| GenreError::ValidationError {
                        message: format!("parsing {}: {}", world_path.display(), e),
                    })?;
                let contribution = if current.is_some() {
                    ContributionKind::Merged
                } else {
                    ContributionKind::Initial
                };
                current = Some(match current {
                    Some(base) => base.merge(val),
                    None => val,
                });
                final_tier = Tier::World;
                final_file = world_path.clone();
                trail.push(MergeStep {
                    tier: Tier::World,
                    file: world_path,
                    span: None,
                    contribution,
                });
            }
        }

        // Culture tier
        if let (Some(world), Some(culture)) = (&ctx.world, &ctx.culture) {
            let culture_path = self
                .root
                .join(&ctx.genre)
                .join("worlds")
                .join(world)
                .join("cultures")
                .join(culture)
                .join(format!("{field_path}.yaml"));
            if let Ok(bytes) = std::fs::read_to_string(&culture_path) {
                let val: T =
                    serde_yaml::from_str(&bytes).map_err(|e| GenreError::ValidationError {
                        message: format!("parsing {}: {}", culture_path.display(), e),
                    })?;
                let contribution = if current.is_some() {
                    ContributionKind::Merged
                } else {
                    ContributionKind::Initial
                };
                current = Some(match current {
                    Some(base) => base.merge(val),
                    None => val,
                });
                final_tier = Tier::Culture;
                final_file = culture_path.clone();
                trail.push(MergeStep {
                    tier: Tier::Culture,
                    file: culture_path,
                    span: None,
                    contribution,
                });
            }
        }

        let value = current.ok_or_else(|| GenreError::ValidationError {
            message: format!("no tier supplied field '{field_path}'"),
        })?;

        let provenance = Provenance {
            source_tier: final_tier,
            source_file: final_file,
            source_span: None,
            merge_trail: trail,
        };

        let elapsed_us = start.elapsed().as_micros() as u64;
        emit_content_resolve_span(
            axis,
            field_path,
            &ctx.genre,
            ctx.world.as_deref(),
            ctx.culture.as_deref(),
            &provenance,
            elapsed_us,
        );

        Ok(Resolved { value, provenance })
    }
}
