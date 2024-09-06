pub struct NewStoredResult {
    pub expr: String,
    pub data: Result<Value>,
    pub concern: Vec<String>
}

PluginAnalysisResults: HashMap<Analysis, NewStoredResult>

struct Analysis {
    publisher: String,
    plugin: String,
    query: Option<String>,
}

// change ReportBuilder, risk threshold is now also a policy expr
//
pub struct ReportBuilder<'sess> {
	session: &'sess Session,
	passing: Vec<PassingAnalysis>,
	failing: Vec<FailingAnalysis>,
	errored: Vec<ErroredAnalysis>,
	risk_threshold: Option<String>, // NEW
	risk_score: Option<f64>,
}

