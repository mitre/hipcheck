// SPDX-License-Identifier: Apache-2.0

pub mod score;

use crate::{
	config::{AttacksConfigQuery, CommitConfigQuery, PracticesConfigQuery},
	data::git::GitProvider,
	metric::MetricProvider,
};

/// Queries about analyses
#[salsa::query_group(AnalysisProviderStorage)]
pub trait AnalysisProvider:
	AttacksConfigQuery + CommitConfigQuery + GitProvider + MetricProvider + PracticesConfigQuery
{
}
