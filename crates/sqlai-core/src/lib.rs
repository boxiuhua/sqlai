//! sqlai-core: 领域类型，无 IO 副作用。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialectKind {
    #[serde(rename = "clickhouse")]
    ClickHouse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub session_id: Uuid,
    pub datasource_id: Uuid,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMeta {
    pub id: Uuid,
    pub datasource_id: Uuid,
    pub db: String,
    pub table: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMeta {
    pub id: Uuid,
    pub table_id: Uuid,
    pub name: String,
    pub data_type: String,
    pub comment: Option<String>,
    pub sample_values: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalContext {
    pub tables: Vec<TableMeta>,
    pub columns: Vec<ColumnMeta>,
    pub business_terms: Vec<BusinessTerm>,
    pub few_shots: Vec<FewShot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessTerm {
    pub term: String,
    pub aliases: Vec<String>,
    pub definition: String,
    pub formula: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FewShot {
    pub question: String,
    pub sql_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntentDecision {
    Direct { hint: String },
    Clarify { prompt: String },
    Reject { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCall {
    pub skill: String,
    pub args: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_serializes_with_kind_tag() {
        let d = IntentDecision::Direct { hint: "h".into() };
        let s = serde_json::to_string(&d).unwrap();
        assert!(s.contains("\"kind\":\"direct\""));
    }

    #[test]
    fn dialect_kind_serializes_to_clickhouse() {
        let d = DialectKind::ClickHouse;
        assert_eq!(serde_json::to_string(&d).unwrap(), "\"clickhouse\"");
    }
}
