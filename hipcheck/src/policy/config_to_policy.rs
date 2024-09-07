// SPDX-License-Identifier: Apache-2.0

//! Code that converts an old TOML-derived Config struct to a new (otherwise KDL-derived) PolicyFile struct
//! This code will eventually be removed once Hipcheck no longer accepts TOML files in lieu of KDL policy files
//! In the meantime, this code exists so Hipcheck can still run using the older configuration format

use super::{policy_file::*, PolicyFile};
use crate::{
	config::{
		ActivityConfig, AffiliationConfig, AttacksConfig, BinaryConfig, ChurnConfig, CommitConfig,
		Config, EntropyConfig, FuzzConfig, IdentityConfig, PracticesConfig, ReviewConfig,
		RiskConfig, TypoConfig,
	},
	error::Result,
	hc_error,
	plugin::PluginVersion,
};

use url::Url;

const PLUGIN_VERSION: &str = "0.1.0";

/// Converts a Config struct to a PolicyFile struct
pub fn config_to_policy(config: Config) -> Result<PolicyFile> {
	// Get the investigate policy
	let investigate = get_investigate(&config.risk)?;

	let mut plugins = PolicyPluginList::new();
	let mut analyze = PolicyAnalyze::new(investigate, None);

	// Add each active analysis to the plugin list and appropriate analysis category
	// Note that while these parse functions return an analysis category, they update the plugins list when they are called
	if let Some(practices) = parse_practices(&mut plugins, &config.analysis.practices) {
		analyze.push(practices);
	}
	if let Some(attacks) = parse_attacks(&mut plugins, &config.analysis.attacks) {
		analyze.push(attacks);
	}

	Ok(PolicyFile { plugins, analyze })
}

/// Converts the overal Config risk score into an overall PolicyFile investigate policy
fn get_investigate(risk: &RiskConfig) -> Result<InvestigatePolicy> {
	let threshold = risk.threshold.into_inner();
	if (0.0..=1.0).contains(&threshold) {
		let expression = format!("(gt {} $)", threshold);
		return Ok(InvestigatePolicy::new(expression));
	}
	Err(hc_error!(
		"The risk value in the config file must be a number between 0 and 1."
	))
}

/// Adds each active practices analysis to the plugins and practices analysis list
fn parse_practices(
	plugins: &mut PolicyPluginList,
	config: &PracticesConfig,
) -> Option<PolicyCategory> {
	// Only add these analysis if this category is active
	if config.active {
		// Cap the weight at 65,533
		let weight = config.weight.try_into().unwrap_or(u16::MAX);
		let mut practices = PolicyCategory::new("practices".to_string(), Some(weight));

		parse_activity(plugins, &mut practices, &config.activity);
		parse_binary(plugins, &mut practices, &config.binary);
		parse_fuzz(plugins, &mut practices, &config.fuzz);
		parse_identity(plugins, &mut practices, &config.identity);
		parse_review(plugins, &mut practices, &config.review);

		return Some(practices);
	}
	None
}

/// Adds the typo analysis and commit sub-category (if each is active) to the plugins and attacks analysis list
fn parse_attacks(plugins: &mut PolicyPluginList, config: &AttacksConfig) -> Option<PolicyCategory> {
	// Only add the analysis and sub-category if this category is active
	if config.active {
		// Cap the weight at 65,533
		let weight = config.weight.try_into().unwrap_or(u16::MAX);
		let mut attacks = PolicyCategory::new("attacks".to_string(), Some(weight));

		parse_typo(plugins, &mut attacks, &config.typo);
		if let Some(commit) = parse_commit(plugins, &config.commit) {
			attacks.push(PolicyCategoryChild::Category(commit));
		}

		return Some(attacks);
	}
	None
}

/// Adds each active commit analysis to the plugins and commit analysis list
fn parse_commit(plugins: &mut PolicyPluginList, config: &CommitConfig) -> Option<PolicyCategory> {
	// Only add these analysis if this category is active
	if config.active {
		// Cap the weight at 65,533
		let weight = config.weight.try_into().unwrap_or(u16::MAX);
		let mut commit = PolicyCategory::new("commit".to_string(), Some(weight));

		parse_affiliation(plugins, &mut commit, &config.affiliation);
		parse_churn(plugins, &mut commit, &config.churn);
		parse_entropy(plugins, &mut commit, &config.entropy);

		return Some(commit);
	}
	None
}

// PANIC: All unwraps are safe because we are providing valid values to the functions

/// Updates the plugin and practices analysis lists with activity policies
fn parse_activity(
	plugins: &mut PolicyPluginList,
	practices: &mut PolicyCategory,
	activity: &ActivityConfig,
) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if activity.active {
		// Cap the weight at 65,533
		let weight = activity.weight.try_into().unwrap_or(u16::MAX);
		let threshold = activity.week_count_threshold;
		let expression = format!("(lte {} $/weeks)", threshold);

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/activity").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-activity.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/activity").unwrap(),
			Some(expression),
			Some(weight),
			None,
		));
		practices.push(analysis);
	}
}

/// Updates the plugin and practices analysis lists with binary policies
fn parse_binary(
	plugins: &mut PolicyPluginList,
	practices: &mut PolicyCategory,
	binary: &BinaryConfig,
) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if binary.active {
		// Cap the weight at 65,533
		let weight = binary.weight.try_into().unwrap_or(u16::MAX);
		let threshold = binary.binary_file_threshold;
		let expression = format!("(eq {} (count $))", threshold);

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/binary").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-binary.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/binary").unwrap(),
			Some(expression),
			Some(weight),
			None,
		));
		practices.push(analysis);
	}
}

/// Updates the plugin and practices analysis lists with fuzz policies
fn parse_fuzz(plugins: &mut PolicyPluginList, practices: &mut PolicyCategory, fuzz: &FuzzConfig) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if fuzz.active {
		// Cap the weight at 65,533
		let weight = fuzz.weight.try_into().unwrap_or(u16::MAX);
		let expression = "(eq #t $)".to_string();

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/fuzz").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-fuzz.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/fuzz").unwrap(),
			Some(expression),
			Some(weight),
			None,
		));
		practices.push(analysis);
	}
}

/// Updates the plugin and practices analysis lists with dentity policies
fn parse_identity(
	plugins: &mut PolicyPluginList,
	practices: &mut PolicyCategory,
	identity: &IdentityConfig,
) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if identity.active {
		// Cap the weight at 65,533
		let weight = identity.weight.try_into().unwrap_or(u16::MAX);
		let threshold = identity.percent_threshold;
		let expression = format!("(lte {} $/pct_match)", threshold);

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/identity").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-identity.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/identity").unwrap(),
			Some(expression),
			Some(weight),
			None,
		));
		practices.push(analysis);
	}
}

/// Updates the plugin and practices analysis lists with review policies
fn parse_review(
	plugins: &mut PolicyPluginList,
	practices: &mut PolicyCategory,
	review: &ReviewConfig,
) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if review.active {
		// Cap the weight at 65,533
		let weight = review.weight.try_into().unwrap_or(u16::MAX);
		let threshold = review.percent_threshold;
		let expression = format!("(lte {} $/pct_reviewed)", threshold);

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/review").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-review.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/review").unwrap(),
			Some(expression),
			Some(weight),
			None,
		));
		practices.push(analysis);
	}
}

/// Updates the plugin and attacks analysis lists with typo policies
fn parse_typo(plugins: &mut PolicyPluginList, attacks: &mut PolicyCategory, typo: &TypoConfig) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if typo.active {
		// Cap the weight at 65,533
		let weight = typo.weight.try_into().unwrap_or(u16::MAX);
		let threshold = typo.count_threshold;
		let expression = format!("(eq {} (count $))", threshold);
		let file = typo.typo_file.clone();
		let mut config = PolicyConfig::new();
		config
			.insert("typo-file".to_string(), format!("./config/{}", file))
			.unwrap();

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/typo").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-typo.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/typo").unwrap(),
			Some(expression),
			Some(weight),
			Some(config),
		));
		attacks.push(analysis);
	}
}

/// Updates the plugin and commit analysis lists with affiliation policies
fn parse_affiliation(
	plugins: &mut PolicyPluginList,
	commit: &mut PolicyCategory,
	affiliation: &AffiliationConfig,
) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if affiliation.active {
		// Cap the weight at 65,533
		let weight = affiliation.weight.try_into().unwrap_or(u16::MAX);
		let threshold = affiliation.count_threshold;
		let expression = format!("(eq {} (count $))", threshold);
		let file = affiliation.orgs_file.clone();
		let mut config = PolicyConfig::new();
		config
			.insert("orgs-file".to_string(), format!("./config/{}", file))
			.unwrap();

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/affiliation").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-affiliation.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/affiliation").unwrap(),
			Some(expression),
			Some(weight),
			Some(config),
		));
		commit.push(analysis);
	}
}

/// Updates the plugin and commit analysis lists with churn policies
fn parse_churn(plugins: &mut PolicyPluginList, commit: &mut PolicyCategory, churn: &ChurnConfig) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if churn.active {
		// Cap the weight at 65,533
		let weight = churn.weight.try_into().unwrap_or(u16::MAX);
		let value_threshold = churn.value_threshold;
		let percent_threshold = churn.percent_threshold;
		let expression = format!(
			"(eq {} (count (filter (gt {}) $)))",
			percent_threshold, value_threshold
		);

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/churn").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-churn.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/churn").unwrap(),
			Some(expression),
			Some(weight),
			None,
		));
		commit.push(analysis);
	}
}

/// Updates the plugin and commit analysis lists with entropy policies
fn parse_entropy(
	plugins: &mut PolicyPluginList,
	commit: &mut PolicyCategory,
	entropy: &EntropyConfig,
) {
	// If the analysis is active, add the appropriate plugin and analysis policies to the policy file
	if entropy.active {
		// Cap the weight at 65,533
		let weight = entropy.weight.try_into().unwrap_or(u16::MAX);
		let value_threshold = entropy.value_threshold;
		let percent_threshold = entropy.percent_threshold;
		let expression = format!(
			"(eq {} (count (filter (gt {}) $)))",
			percent_threshold, value_threshold
		);

		// Add the plugin
		let plugin = PolicyPlugin::new(
			PolicyPluginName::new("mitre/entropy").unwrap(),
			PluginVersion::new(PLUGIN_VERSION.to_string()),
			Some(
				Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-entropy.kdl",
				)
				.unwrap(),
			),
		);
		plugins.push(plugin);

		// Add the analysis
		let analysis = PolicyCategoryChild::Analysis(PolicyAnalysis::new(
			PolicyPluginName::new("mitre/entropy").unwrap(),
			Some(expression),
			Some(weight),
			None,
		));
		commit.push(analysis);
	}
}
