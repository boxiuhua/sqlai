-- 启用 pgvector
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- 数据源
CREATE TABLE IF NOT EXISTS datasource (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL,           -- v1.0 仅 'clickhouse'
    host            TEXT NOT NULL,
    port            INT  NOT NULL,
    db              TEXT NOT NULL,
    user_name       TEXT NOT NULL,
    secret_ref      TEXT NOT NULL,           -- 引用环境变量 / vault key，不直接存密码
    readonly        BOOLEAN NOT NULL DEFAULT TRUE,
    settings        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (name)
);

-- schema 元数据：表
CREATE TABLE IF NOT EXISTS table_meta (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    datasource_id   UUID NOT NULL REFERENCES datasource(id) ON DELETE CASCADE,
    db              TEXT NOT NULL,
    table_name      TEXT NOT NULL,
    comment         TEXT,
    row_count_est   BIGINT,
    embedding       VECTOR(1024),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (datasource_id, db, table_name)
);
CREATE INDEX IF NOT EXISTS table_meta_embedding_idx
    ON table_meta USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- schema 元数据：列
CREATE TABLE IF NOT EXISTS column_meta (
    id                 UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    table_id           UUID NOT NULL REFERENCES table_meta(id) ON DELETE CASCADE,
    name               TEXT NOT NULL,
    data_type          TEXT NOT NULL,
    comment            TEXT,
    sample_values      JSONB NOT NULL DEFAULT '[]'::jsonb,
    distinct_count_est BIGINT,
    embedding          VECTOR(1024),
    UNIQUE (table_id, name)
);
CREATE INDEX IF NOT EXISTS column_meta_embedding_idx
    ON column_meta USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- 业务知识：词表
CREATE TABLE IF NOT EXISTS business_term (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    term        TEXT NOT NULL,
    aliases     TEXT[] NOT NULL DEFAULT '{}',
    definition  TEXT NOT NULL,
    formula     TEXT,
    embedding   VECTOR(1024),
    UNIQUE (term)
);
CREATE INDEX IF NOT EXISTS business_term_embedding_idx
    ON business_term USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- 业务知识：指标定义
CREATE TABLE IF NOT EXISTS metric_def (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    dimension_keys  TEXT[] NOT NULL DEFAULT '{}',
    measure_sql     TEXT NOT NULL,
    owner           TEXT,
    embedding       VECTOR(1024),
    UNIQUE (name)
);
CREATE INDEX IF NOT EXISTS metric_def_embedding_idx
    ON metric_def USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- 会话与历史
CREATE TABLE IF NOT EXISTS session (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         TEXT NOT NULL,
    datasource_id   UUID REFERENCES datasource(id),
    title           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS session_user_idx ON session(user_id);

CREATE TABLE IF NOT EXISTS message (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    session_id      UUID NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,            -- user / assistant / system
    content         JSONB NOT NULL,           -- 原始问题 / SkillCall / 解释 / 摘要
    plan            JSONB,                    -- AnalysisPlan
    chart_spec      JSONB,
    rows_returned   INT,
    latency_ms      INT,
    parent_id       UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS message_session_created_idx ON message(session_id, created_at);

-- few-shot
CREATE TABLE IF NOT EXISTS few_shot (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    question        TEXT NOT NULL,
    skill_call      JSONB NOT NULL,
    sql_text        TEXT NOT NULL,
    datasource_id   UUID REFERENCES datasource(id),
    vote            INT NOT NULL DEFAULT 0,
    embedding       VECTOR(1024),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS few_shot_embedding_idx
    ON few_shot USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);
