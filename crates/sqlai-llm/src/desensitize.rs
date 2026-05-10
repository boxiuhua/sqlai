//! 进入 LLM 的上下文必经此层脱敏。

use serde_json::Value;
use sqlai_core::{ColumnMeta, RetrievalContext, TableMeta};

/// 已脱敏的上下文。无公开构造函数 —— 只能由 `mask()` 产出。
#[derive(Debug, Clone)]
pub struct MaskedContext {
    inner: RetrievalContext,
}

impl MaskedContext {
    /// 暴露已脱敏后的上下文以便 LlmProvider 读取。
    /// 注意：故意不实现 `AsRef<RetrievalContext>` —— 那会让消费者通过 trait 路径
    /// 拿到引用，模糊了"必须显式承认正在读取脱敏数据"这一安全意图。
    pub fn inner(&self) -> &RetrievalContext {
        &self.inner
    }
}

/// 默认敏感列名规则（小写匹配）。后续可由配置覆盖。
const SENSITIVE_NAME_HINTS: &[&str] = &[
    "phone", "mobile", "tel", "email", "mail", "id_card", "idcard", "passport", "password",
    "passwd", "secret", "token", "address", "addr", "bank", "card_no", "cardno",
];

pub fn mask(ctx: RetrievalContext) -> MaskedContext {
    let RetrievalContext {
        tables,
        columns,
        business_terms,
        few_shots,
    } = ctx;

    let columns = columns.into_iter().map(mask_column).collect();
    MaskedContext {
        inner: RetrievalContext {
            tables: tables.into_iter().map(mask_table).collect(),
            columns,
            business_terms,
            few_shots,
        },
    }
}

fn mask_table(t: TableMeta) -> TableMeta {
    t // 表名暂不脱敏
}

fn mask_column(mut c: ColumnMeta) -> ColumnMeta {
    if is_sensitive(&c.name) {
        c.sample_values = c
            .sample_values
            .into_iter()
            .map(|v| mask_value(&v))
            .collect();
    }
    c
}

fn is_sensitive(name: &str) -> bool {
    let lower = name.to_lowercase();
    SENSITIVE_NAME_HINTS.iter().any(|h| lower.contains(h))
}

fn mask_value(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(mask_string(s)),
        _ => Value::String("***".into()),
    }
}

fn mask_string(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    if n <= 2 {
        return "*".repeat(n);
    }
    let keep_head = chars[0];
    let keep_tail = chars[n - 1];
    format!("{}{}{}", keep_head, "*".repeat(n - 2), keep_tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn col(name: &str, sample: Vec<Value>) -> ColumnMeta {
        ColumnMeta {
            id: Uuid::new_v4(),
            table_id: Uuid::new_v4(),
            name: name.into(),
            data_type: "String".into(),
            comment: None,
            sample_values: sample,
        }
    }

    #[test]
    fn mask_string_keeps_head_and_tail() {
        assert_eq!(mask_string("alice"), "a***e");
        assert_eq!(mask_string("ab"), "**");
        assert_eq!(mask_string("a"), "*");
    }

    #[test]
    fn sensitive_columns_get_masked() {
        let ctx = RetrievalContext {
            tables: vec![],
            columns: vec![
                col(
                    "phone_number",
                    vec![json!("13800138000"), json!("18811112222")],
                ),
                col("user_name", vec![json!("alice"), json!("bob")]),
            ],
            business_terms: vec![],
            few_shots: vec![],
        };
        let m = mask(ctx);
        let inner = m.inner();

        let phone = inner
            .columns
            .iter()
            .find(|c| c.name == "phone_number")
            .unwrap();
        assert_eq!(
            phone.sample_values,
            vec![json!("1*********0"), json!("1*********2")]
        );

        let name = inner
            .columns
            .iter()
            .find(|c| c.name == "user_name")
            .unwrap();
        assert_eq!(name.sample_values, vec![json!("alice"), json!("bob")]);
    }

    #[test]
    fn non_string_sensitive_value_replaced_with_stars() {
        let ctx = RetrievalContext {
            tables: vec![],
            columns: vec![col("id_card", vec![json!(110101199001011234_i64)])],
            business_terms: vec![],
            few_shots: vec![],
        };
        let m = mask(ctx);
        assert_eq!(m.inner().columns[0].sample_values, vec![json!("***")]);
    }
}
