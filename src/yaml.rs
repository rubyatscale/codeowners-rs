//! Options keep serde_yaml 0.9's semantics where serde-saphyr allows it: these files are written for Ruby's Psych,
//! and most package.yml keys are ones we never read, so rejecting a file over one would abort the whole run.
use error_stack::Report;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_saphyr::{DuplicateKeyPolicy, MergeKeyPolicy, NonFiniteFloatPolicy, Options, budget::BudgetBreach};
use std::path::PathBuf;

/// Default for the `yaml_max_nodes` config key. Real files have at most a few thousand nodes.
pub(crate) const DEFAULT_MAX_NODES: usize = 20_000;

/// `yaml_max_nodes` for team files and package.yml, and the config file to raise it in, which the hint names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NodeLimit {
    pub(crate) max_nodes: usize,
    pub(crate) config_path: PathBuf,
}

#[cfg(test)]
impl Default for NodeLimit {
    fn default() -> Self {
        Self {
            max_nodes: DEFAULT_MAX_NODES,
            config_path: PathBuf::from(crate::config::DEFAULT_PATH),
        }
    }
}

fn options(max_nodes: usize) -> Options {
    serde_saphyr::options! {
        // LastWins re-buffers nested mappings once per level, so the default budget lets a 500 KB file take gigabytes.
        // Real files nest about 5 deep; the depth limit also prevents stack overflow.
        // The other caps grow with `yaml_max_nodes`, so raising it is all a larger file needs.
        // At the default limit, the event cap is serde-saphyr's own default.
        budget: serde_saphyr::budget! {
            max_depth: 16,
            max_nodes: max_nodes,
            max_events: max_nodes.saturating_mul(50),
            max_total_scalar_bytes: max_nodes.saturating_mul(256),
            max_recorded_anchor_events: max_nodes.saturating_mul(5),
            max_recorded_anchor_bytes: max_nodes.saturating_mul(64),
        },
        // The default rejects a repeated key anywhere, including in values we ignore. Last wins, as in Psych.
        duplicate_keys: DuplicateKeyPolicy::LastWins,
        // serde_yaml treated `<<` as an ordinary key.
        merge_keys: MergeKeyPolicy::AsOrdinary,
        // Ignored values are still parsed, and the default rejects `.inf`, `.nan` and `1e999` in them.
        non_finite_float_policy: NonFiniteFloatPolicy::PassThrough,
        // serde_yaml neither decoded nor validated `!!binary`.
        ignore_binary_tag_for_string: true,
        // serde_yaml rejected YAML 1.1 booleans such as `yes` and `off`.
        strict_booleans: true,
        // Snippets add several lines to each error, which serde_yaml kept to one.
        with_snippet: false,
    }
}

/// Takes bytes, not a reader: `from_reader` transcodes after a BOM, replacing invalid UTF-8 and accepting UTF-16, unlike serde_yaml.
pub(crate) fn from_slice<T: DeserializeOwned>(bytes: &[u8], max_nodes: usize) -> Result<T, serde_saphyr::Error> {
    serde_saphyr::from_slice_with_options(bytes, options(max_nodes))
}

/// [`from_slice`] for team files and package.yml, whose size limits come from the `yaml_max_nodes` config key: a file
/// over them gets a hint naming that key and the config file.
pub(crate) fn from_project_slice<T: DeserializeOwned>(bytes: &[u8], node_limit: &NodeLimit) -> Result<T, Report<YamlError>> {
    from_slice(bytes, node_limit.max_nodes).map_err(|error| {
        let mut original = &error;
        while let serde_saphyr::Error::AliasError { error, .. } = original {
            original = error;
        }
        let too_large = matches!(
            original,
            serde_saphyr::Error::Budget {
                breach: BudgetBreach::Nodes { .. }
                    | BudgetBreach::Events { .. }
                    | BudgetBreach::ScalarBytes { .. }
                    | BudgetBreach::RecordedAnchorEvents { .. }
                    | BudgetBreach::RecordedAnchorBytes { .. },
                ..
            }
        );
        let report = Report::new(YamlError(error));
        if too_large {
            report.attach(format!(
                "raise `yaml_max_nodes` (now {}) in {} to allow larger files",
                node_limit.max_nodes,
                node_limit.config_path.display()
            ))
        } else {
            report
        }
    })
}

/// A serde-saphyr error with no `source()`. An alias error's message already includes the error it wraps, and
/// error_stack would print that error again as its source.
#[derive(Debug)]
pub(crate) struct YamlError(pub(crate) serde_saphyr::Error);

impl std::fmt::Display for YamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl core::error::Error for YamlError {}

/// Reads an empty, `~` or `null` value as `""` instead of failing. serde_yaml accepted these, keeping `~` and `null` as text.
pub(crate) fn string_or_null<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// [`string_or_null`] for each item of a list. Reads an empty, `~` or `null` list as empty itself, since serde-saphyr 2.1
/// stops doing that for a `Vec` (serde-saphyr#233), and `unowned_globs:` with no value is common.
pub(crate) fn strings_or_nulls<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<Option<String>>>::deserialize(deserializer)?
        .unwrap_or_default()
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::deserializers::RubyPackage;

    fn owner(yaml: &[u8]) -> Result<Option<String>, serde_saphyr::Error> {
        from_slice::<RubyPackage>(yaml, DEFAULT_MAX_NODES).map(|package| package.owner)
    }

    #[test]
    fn test_ignores_unusual_values_in_unknown_keys() {
        for unknown in [
            "<<: 5",
            "error_budget: .inf",
            "sample_rate: .nan",
            "huge: 1e999",
            "blob: !!binary not base64",
            "legacy: !ruby/object:OpenStruct {table: {}}",
        ] {
            let yaml = format!("owner: TeamA\n{unknown}\n");
            assert_eq!(owner(yaml.as_bytes()).expect(unknown), Some("TeamA".to_string()), "{unknown}");
        }
    }

    #[test]
    fn test_reads_empty_list_as_empty() {
        #[derive(Deserialize)]
        struct Globs {
            #[serde(deserialize_with = "strings_or_nulls")]
            globs: Vec<String>,
        }

        for yaml in ["globs:\n", "globs: ~\n", "globs: null\n", "globs: []\n"] {
            let parsed: Globs = from_slice(yaml.as_bytes(), DEFAULT_MAX_NODES).expect(yaml);
            assert!(parsed.globs.is_empty(), "{yaml:?}");
        }
    }

    #[test]
    fn test_strings_or_nulls_reads_null_as_empty_list() {
        use serde::de::IntoDeserializer;
        // serde-saphyr 2.0 would read null as an empty list even without the helper; serde's unit deserializer does not.
        let null: serde::de::value::UnitDeserializer<serde::de::value::Error> = ().into_deserializer();
        assert_eq!(strings_or_nulls(null).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn test_does_not_apply_merge_keys() {
        let yaml = "defaults: &defaults\n  owner: TeamA\n<<: *defaults\nmetadata:\n  <<: {owner: TeamA}\n";
        let package: RubyPackage = from_slice(yaml.as_bytes(), DEFAULT_MAX_NODES).unwrap();
        assert_eq!(package.owner, None);
        assert_eq!(package.metadata.and_then(|metadata| metadata.owner), None);
    }

    #[test]
    fn test_reads_utf8_bom_before_several_keys() {
        assert_eq!(
            owner("\u{feff}enforce_dependencies: true\nowner: TeamA\n".as_bytes()).unwrap(),
            Some("TeamA".to_string())
        );
    }

    #[test]
    fn test_rejects_invalid_utf8_after_bom() {
        assert!(owner(b"\xef\xbb\xbfowner: Caf\xe9\n").is_err());
    }

    #[test]
    fn test_rejects_deeply_nested_values() {
        let nested = |lists: usize| format!("owner: TeamA\nignored: {}{}\n", "[".repeat(lists), "]".repeat(lists));

        // 16 levels, counting the top-level mapping, are allowed.
        assert_eq!(owner(nested(15).as_bytes()).unwrap(), Some("TeamA".to_string()));
        assert!(matches!(
            owner(nested(16).as_bytes()),
            Err(serde_saphyr::Error::Budget {
                breach: BudgetBreach::Depth { .. },
                ..
            })
        ));

        // Raising `yaml_max_nodes` does not raise the depth limit, so its breach gets no hint.
        let Err(report) = from_project_slice::<RubyPackage>(nested(16).as_bytes(), &NodeLimit::default()) else {
            panic!("17 levels should be rejected");
        };
        assert!(!format!("{report:?}").contains("yaml_max_nodes"), "{report:?}");
    }

    #[test]
    fn test_rejects_too_many_nodes() {
        let owner_with_items = |count: usize, max_nodes: usize| {
            let yaml = format!("owner: TeamA\nignored: [{}]\n", vec!["x"; count].join(", "));
            from_slice::<RubyPackage>(yaml.as_bytes(), max_nodes).map(|package| package.owner)
        };

        assert_eq!(owner_with_items(10_000, DEFAULT_MAX_NODES).unwrap(), Some("TeamA".to_string()));
        assert!(matches!(
            owner_with_items(100_000, DEFAULT_MAX_NODES),
            Err(serde_saphyr::Error::Budget { .. })
        ));
        assert_eq!(owner_with_items(100_000, 200_000).unwrap(), Some("TeamA".to_string()));
        assert!(matches!(owner_with_items(1_000, 500), Err(serde_saphyr::Error::Budget { .. })));
    }

    #[test]
    fn test_anchor_copy_limits_grow_with_max_nodes() {
        // Ten nested anchored lists copy each of their 10,001 items into ten anchor buffers.
        let nested_anchors = format!(
            "owner: TeamA\nignored: {}{}{}\n",
            (0..10).map(|i| format!("&a{i} [")).collect::<String>(),
            vec!["x"; 10_001].join(", "),
            "]".repeat(10)
        );
        // The escape makes the scalar owned, so its 1.3 MB count toward the anchor-copy byte limit.
        let owned_anchor = format!("owner: TeamA\nbig: &a \"{}\\t\"\n", "x".repeat(1_300_000));

        for yaml in [&nested_anchors, &owned_anchor] {
            let Err(report) = from_project_slice::<RubyPackage>(yaml.as_bytes(), &NodeLimit::default()) else {
                panic!("should exceed an anchor-copy limit at the default yaml_max_nodes");
            };
            assert!(
                matches!(
                    &report.current_context().0,
                    serde_saphyr::Error::Budget {
                        breach: BudgetBreach::RecordedAnchorEvents { .. } | BudgetBreach::RecordedAnchorBytes { .. },
                        ..
                    }
                ),
                "{report:?}"
            );
            assert!(format!("{report:?}").contains("raise `yaml_max_nodes`"), "{report:?}");
            assert_eq!(
                from_slice::<RubyPackage>(yaml.as_bytes(), 40_000).unwrap().owner,
                Some("TeamA".to_string())
            );
        }
    }

    #[test]
    fn test_event_limit_grows_with_max_nodes() {
        // A node yields up to about three parser events, so a fixed event cap such as serde-saphyr's default of 1,000,000
        // would reject files that a raised `yaml_max_nodes` allows.
        let budget = options(700_000).budget.unwrap();
        assert!(budget.max_events > serde_saphyr::Budget::default().max_events);
    }

    #[test]
    fn test_event_limit_is_serde_saphyr_default_at_default_max_nodes() {
        // No file reaches the event cap before another cap unless the limit is 0, where every multiplier gives 0, so no
        // parse can pin the multiplier.
        let budget = options(DEFAULT_MAX_NODES).budget.unwrap();
        assert_eq!(budget.max_events, serde_saphyr::Budget::default().max_events);
    }

    #[test]
    fn test_event_limit_breach_names_yaml_max_nodes() {
        // The event cap is reached before the node cap only at a limit of 0.
        let node_limit = NodeLimit {
            max_nodes: 0,
            ..Default::default()
        };
        let Err(report) = from_project_slice::<RubyPackage>(b"owner: TeamA\n", &node_limit) else {
            panic!("a yaml_max_nodes of 0 should reject every file");
        };
        assert!(
            matches!(
                &report.current_context().0,
                serde_saphyr::Error::Budget {
                    breach: BudgetBreach::Events { .. },
                    ..
                }
            ),
            "{report:?}"
        );
        assert!(format!("{report:?}").contains("raise `yaml_max_nodes` (now 0)"), "{report:?}");
    }

    #[test]
    fn test_scalar_byte_limit_grows_with_max_nodes() {
        let yaml = format!("owner: TeamA\nignored: [{}]\n", vec!["x".repeat(400); 15_000].join(", "));

        let Err(report) = from_project_slice::<RubyPackage>(yaml.as_bytes(), &NodeLimit::default()) else {
            panic!("15,000 strings of 400 bytes should exceed the default scalar-byte limit");
        };
        assert!(matches!(
            &report.current_context().0,
            serde_saphyr::Error::Budget {
                breach: BudgetBreach::ScalarBytes { .. },
                ..
            }
        ));
        assert!(format!("{report:?}").contains("raise `yaml_max_nodes`"), "{report:?}");
        assert_eq!(
            from_slice::<RubyPackage>(yaml.as_bytes(), 40_000).unwrap().owner,
            Some("TeamA".to_string())
        );

        let aliased = format!(
            "owner: TeamA\nbig: &a \"{}\"\nignored: [{}]\n",
            "x".repeat(1_000_000),
            ["*a"; 8].join(", ")
        );
        let Err(report) = from_project_slice::<RubyPackage>(aliased.as_bytes(), &NodeLimit::default()) else {
            panic!("eight aliases of a 1 MB string should exceed the default scalar-byte limit");
        };
        assert!(format!("{report:?}").contains("raise `yaml_max_nodes`"), "{report:?}");
    }

    #[test]
    fn test_reports_an_alias_error_once() {
        let Err(report) = from_project_slice::<RubyPackage>(b"x: &x [a]\nowner: *x\n", &NodeLimit::default()) else {
            panic!("a sequence is not a valid owner");
        };
        let message = format!("{report:?}");
        assert_eq!(message.matches("expected string scalar").count(), 1, "{message}");
    }
}
