//! sqlai-skills：AnalysisSkill 抽象 + 内置 skill 集。

pub mod compute;
pub mod descriptive;
pub mod diagnostic;
pub mod error;
pub mod ml;
pub mod plan;
pub mod render;

pub use error::SkillError;
pub use plan::{
    AnalysisPlan, AnalysisStep, ChartHint, ChartKind, ComputeFn, ComputeStep, MlStep, SqlStep,
};

use serde::{Deserialize, Serialize};
use sqlai_core::RetrievalContext;
use std::collections::BTreeMap;
use std::sync::Arc;

/// 给 LLM 看的工具描述，OpenAI tools 接口兼容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSchema {
    pub name: String,
    pub description: String,
    /// JSON Schema 描述参数。
    pub parameters: serde_json::Value,
}

pub trait AnalysisSkill: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> SkillSchema;
    fn plan(
        &self,
        args: &serde_json::Value,
        ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError>;
}

#[derive(Default)]
pub struct SkillRegistry {
    skills: BTreeMap<&'static str, Arc<dyn AnalysisSkill>>,
}

impl SkillRegistry {
    pub fn empty() -> Self {
        Self {
            skills: BTreeMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut r = Self::empty();
        r.register(Arc::new(descriptive::metric_overview::MetricOverview));
        r.register(Arc::new(descriptive::topn::TopN));
        r.register(Arc::new(descriptive::compare_period::ComparePeriod));
        r.register(Arc::new(descriptive::share_breakdown::ShareBreakdown));
        r.register(Arc::new(descriptive::trend_segment::TrendSegment));
        r.register(Arc::new(diagnostic::drill_down::DrillDown));
        r.register(Arc::new(diagnostic::correlation_matrix::CorrelationMatrix));
        r.register(Arc::new(diagnostic::distribution_shift::DistributionShift));
        r.register(Arc::new(compute::forecast_simple::ForecastSimple));
        r.register(Arc::new(ml::cluster_kmeans::ClusterKmeans));
        r.register(Arc::new(ml::classify_logreg::ClassifyLogreg));
        r
    }

    pub fn register(&mut self, skill: Arc<dyn AnalysisSkill>) {
        self.skills.insert(skill.name(), skill);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn AnalysisSkill>> {
        self.skills.get(name).cloned()
    }

    pub fn all_schemas(&self) -> Vec<SkillSchema> {
        self.skills.values().map(|s| s.schema()).collect()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.skills.keys().copied().collect()
    }
}
