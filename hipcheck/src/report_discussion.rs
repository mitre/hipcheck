// X_analysis() functions return HcAnalysisReport, contains outcome (Result<Value>), and concerns

// HCAnalysisValue - simple type system, ought to be replaced with serde::Value (?)

pub struct HCAnalysisReport {
	pub outcome: HCAnalysisOutcome,
	pub concerns: Vec<Concern>,
}

pub enum HCAnalysisOutcome {
	Error(HCAnalysisError),
	Completed(HCAnalysisValue),
}

// HCAnalysisReport + initial threshold spec combined into a HCStoredResult
//      passes along concerns from HCAnalysisReport,
//      combines predicate info + HCAnalysisOutcome::Completed(HCAnalysisValue)
//          to store (but not immediately score) the pass/fail result

pub struct HCStoredResult {
	pub result: Result<Arc<Predicate>>,
	pub concerns: Vec<Concern>,
}

// In crate::score::score_results(), create an AnalysisResults and fill

pub struct AnalysisResults {
	pub table: HashMap<String, HCStoredResult>,
}

// score_results() uses ScoreTree::score() to get score, packages that with
// AnalysisResults into ScoringResults

pub struct ScoringResults {
	pub results: AnalysisResults,
	pub score: Score,
}

// Session + Scoring results then passed to `report_builder::build_report -> Report`

pub enum Analysis {
	Activity { scoring: Count },
	Affiliation { scoring: Count },
	Binary { scoring: Count },
	Churn { scoring: Percent },
	Entropy { scoring: Percent },
	Identity { scoring: Percent },
	Fuzz { scoring: Exists },
	Review { scoring: Percent },
	Typo { scoring: Count },
}

pub struct ReportBuilder<'sess> {
	session: &'sess Session,
	passing: Vec<PassingAnalysis>,
	failing: Vec<FailingAnalysis>,
	errored: Vec<ErroredAnalysis>,
	risk_threshold: Option<f64>,
	risk_score: Option<f64>,
}

impl ReportBuilder {
	pub fn add_analysis(&mut self, analysis: Analysis, concerns: Vec<Concern>)
		-> Result<&mut Self>;
}

pub enum AnalysisIdent {
	Activity,
	Affiliation,
	Binary,
	Churn,
	Entropy,
	Identity,
	Fuzz,
	Review,
	Typo,
}

pub struct PassingAnalysis(Analysis);

pub struct FailingAnalysis {
	analysis: Analysis,
	concerns: Vec<Concern>,
}

pub struct ErroredAnalysis {
	analysis: AnalysisIdent,
	error: ErrorReport,
}

// NOTES
// - plugin system doesn't offer a way for analyses to specify units, etc for printing
// - report::Analysis and AnalysisIdent need to be refactored to not hard-code legacy analyses
