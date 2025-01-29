// SPDX-License-Identifier: Apache-2.0

//! Tests of policy file parsing functions
#[cfg(test)]
mod test {
	use crate::{
		config::Config,
		plugin::PluginVersion,
		policy::{config_to_policy::config_to_policy, policy_file::*, PolicyFile, PolicyPatchList},
		util::kdl::ParseKdlNode,
	};

	use kdl::KdlNode;
	use pathbuf::pathbuf;
	use serde_json::Value;
	use std::{env, str::FromStr};
	use url::Url;

	#[test]
	fn test_parsing_plugin() {
		let data = r#"plugin "mitre/activity" version="0.1.0" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-activity.kdl""#;
		let node = KdlNode::from_str(data).unwrap();

		let expected = PolicyPlugin::new(
			PolicyPluginName::new("mitre/activity").unwrap(),
			PluginVersion::new("0.1.0".to_string()),
			Some(ManifestLocation::Url(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-activity.kdl",
				)
				.unwrap(),
			)),
		);

		assert_eq!(expected, PolicyPlugin::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parsing_plugin_list() {
		let data = r#"plugins {
        plugin "mitre/activity" version="0.1.0" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-activity.kdl"
        plugin "mitre/binary" version="0.1.1" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-binary.kdl"
        }"#;
		let node = KdlNode::from_str(data).unwrap();

		let mut expected = PolicyPluginList::new();
		expected.push(PolicyPlugin::new(
			PolicyPluginName::new("mitre/activity").unwrap(),
			PluginVersion::new("0.1.0".to_string()),
			Some(ManifestLocation::Url(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-activity.kdl",
				)
				.unwrap(),
			)),
		));
		expected.push(PolicyPlugin::new(
			PolicyPluginName::new("mitre/binary").unwrap(),
			PluginVersion::new("0.1.1".to_string()),
			Some(ManifestLocation::Url(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-binary.kdl",
				)
				.unwrap(),
			)),
		));

		assert_eq!(expected, PolicyPluginList::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parsing_investigate_policy() {
		let data = r#"investigate policy="(gt 0.5 $)""#;
		let node = KdlNode::from_str(data).unwrap();

		let expected = InvestigatePolicy("(gt 0.5 $)".to_string());

		assert_eq!(expected, InvestigatePolicy::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parsing_investigate_if_fail() {
		let data = r#"investigate-if-fail "mitre/typo" "mitre/binary""#;
		let node = KdlNode::from_str(data).unwrap();

		let mut expected = InvestigateIfFail::new();
		expected.push("mitre/typo");
		expected.push("mitre/binary");

		assert_eq!(expected, InvestigateIfFail::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parsing_analysis_weight() {
		let data = r#"analysis "mitre/typo" policy="(eq 0 (count $))" weight=3"#;
		let node = KdlNode::from_str(data).unwrap();

		let mut config = PolicyConfig::new();
		config
			.insert(
				"typo-file".to_string(),
				Value::String("./config/typo.kdl".to_string()),
			)
			.unwrap();

		let expected = PolicyAnalysis::new(
			PolicyPluginName::new("mitre/typo").unwrap(),
			Some("(eq 0 (count $))".to_string()),
			Some(3),
			None,
		);

		assert_eq!(expected, PolicyAnalysis::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parsing_analysis_config() {
		let data = r#"analysis "mitre/typo" policy="(eq 0 (count $))" {
            typo-file "./config/typo.kdl"
        }"#;
		let node = KdlNode::from_str(data).unwrap();

		let mut config = PolicyConfig::new();
		config
			.insert(
				"typo-file".to_string(),
				Value::String("./config/typo.kdl".to_string()),
			)
			.unwrap();

		let expected = PolicyAnalysis::new(
			PolicyPluginName::new("mitre/typo").unwrap(),
			Some("(eq 0 (count $))".to_string()),
			None,
			Some(config),
		);

		assert_eq!(expected, PolicyAnalysis::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parsing_analysis_multiple_configs() {
		let data = r#"analysis "mitre/typo" policy="(eq 0 (count $))" weight=3 {
            typo-file "./config/typo.kdl"
            typo-file-2 "./config/typo2.kdl"
        }"#;
		let node = KdlNode::from_str(data).unwrap();

		let mut config = PolicyConfig::new();
		config
			.insert(
				"typo-file".to_string(),
				Value::String("./config/typo.kdl".to_string()),
			)
			.unwrap();
		config
			.insert(
				"typo-file-2".to_string(),
				Value::String("./config/typo2.kdl".to_string()),
			)
			.unwrap();

		let expected = PolicyAnalysis::new(
			PolicyPluginName::new("mitre/typo").unwrap(),
			Some("(eq 0 (count $))".to_string()),
			Some(3),
			Some(config),
		);

		assert_eq!(expected, PolicyAnalysis::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parse_analyze() {
		let data = r#"analyze {
            investigate policy="(gt 0.5 $)"
            investigate-if-fail "mitre/typo" "mitre/binary"

            category "practices" weight=2 {
                analysis "mitre/activity" policy="(lte 52 $/weeks)" weight=3
                analysis "mitre/binary" policy="(eq 0 (count $))"
            }

            category "attacks" {
                analysis "mitre/typo" policy="(eq 0 (count $))" {
                    typo-file "./config/typo.kdl"
                }

                category "commit" {
                    analysis "mitre/affiliation" policy="(eq 0 (count $))" {
                    orgs-file "./config/orgs.kdl"
                    }

                    analysis "mitre/entropy" policy="(eq 0 (count (filter (gt 8.0) $)))"
                    analysis "mitre/churn" policy="(eq 0 (count (filter (gt 8.0) $)))"
                }
            }
        }"#;
		let node = KdlNode::from_str(data).unwrap();

		let investigate_policy = InvestigatePolicy("(gt 0.5 $)".to_string());

		let mut if_fail = InvestigateIfFail::new();
		if_fail.push("mitre/typo");
		if_fail.push("mitre/binary");

		let mut practices = PolicyCategory::new("practices".to_string(), Some(2));
		practices.push(PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/activity").unwrap(),
			Some("(lte 52 $/weeks)".to_string()),
			Some(3),
			None,
		)));
		practices.push(PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/binary").unwrap(),
			Some("(eq 0 (count $))".to_string()),
			None,
			None,
		)));

		let mut affiliation_config = PolicyConfig::new();
		affiliation_config
			.insert(
				"orgs-file".to_string(),
				Value::String("./config/orgs.kdl".to_string()),
			)
			.unwrap();

		let mut typo_config = PolicyConfig::new();
		typo_config
			.insert(
				"typo-file".to_string(),
				Value::String("./config/typo.kdl".to_string()),
			)
			.unwrap();

		let mut commit = PolicyCategory::new("commit".to_string(), None);
		commit.push(PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/affiliation").unwrap(),
			Some("(eq 0 (count $))".to_string()),
			None,
			Some(affiliation_config),
		)));
		commit.push(PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/entropy").unwrap(),
			Some("(eq 0 (count (filter (gt 8.0) $)))".to_string()),
			None,
			None,
		)));
		commit.push(PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/churn").unwrap(),
			Some("(eq 0 (count (filter (gt 8.0) $)))".to_string()),
			None,
			None,
		)));

		let mut attacks = PolicyCategory::new("attacks".to_string(), None);
		attacks.push(PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/typo").unwrap(),
			Some("(eq 0 (count $))".to_string()),
			None,
			Some(typo_config),
		)));
		attacks.push(PolicyCategoryChild::Category(commit));

		let mut expected = PolicyAnalyze::new(investigate_policy, Some(if_fail));
		expected.push(practices);
		expected.push(attacks);

		assert_eq!(expected, PolicyAnalyze::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parse_policy_file() {
		let data = r#"plugins {
            plugin "mitre/activity" version="0.1.0" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-activity.kdl"
            plugin "mitre/binary" version="0.1.1" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-binary.kdl"
        }

        analyze {
            investigate policy="(gt 0.5 $)"
            investigate-if-fail "mitre/binary"

            category "practices" {
                analysis "mitre/activity" policy="(lte 52 $/weeks)" weight=3
                analysis "mitre/binary" policy="(eq 0 (count $))"
            }
        }"#;

		let mut plugins = PolicyPluginList::new();
		plugins.push(PolicyPlugin::new(
			PolicyPluginName::new("mitre/activity").unwrap(),
			PluginVersion::new("0.1.0".to_string()),
			Some(ManifestLocation::Url(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-activity.kdl",
				)
				.unwrap(),
			)),
		));
		plugins.push(PolicyPlugin::new(
			PolicyPluginName::new("mitre/binary").unwrap(),
			PluginVersion::new("0.1.1".to_string()),
			Some(ManifestLocation::Url(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-binary.kdl",
				)
				.unwrap(),
			)),
		));

		let investigate_policy = InvestigatePolicy("(gt 0.5 $)".to_string());

		let mut if_fail = InvestigateIfFail::new();
		if_fail.push("mitre/binary");

		let mut practices = PolicyCategory::new("practices".to_string(), None);
		practices.push(PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/activity").unwrap(),
			Some("(lte 52 $/weeks)".to_string()),
			Some(3),
			None,
		)));
		practices.push(PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/binary").unwrap(),
			Some("(eq 0 (count $))".to_string()),
			None,
			None,
		)));

		let mut analyze = PolicyAnalyze::new(investigate_policy, Some(if_fail));
		analyze.push(practices);

		let expected = PolicyFile {
			plugins,
			patch: PolicyPatchList::default(),
			analyze,
		};

		assert_eq!(expected, PolicyFile::from_str(data).unwrap())
	}

	#[test]
	fn test_config_to_policy() {
		let config_path = pathbuf![&env::current_dir().unwrap(), "..", "config"];
		let config = Config::load_from(&config_path).unwrap();
		let test_cfg_path = pathbuf!["./config"];
		let policy_file = config_to_policy(config, &test_cfg_path).unwrap();

		let expected_path = pathbuf![
			&env::current_dir().unwrap(),
			"src",
			"policy",
			"test_example.kdl"
		];

		let expected = PolicyFile::load_from(&expected_path).unwrap();

		assert_eq!(expected, policy_file)
	}
}
