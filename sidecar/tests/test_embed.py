"""/embed: 不真的下载 BGE-M3；用 stub 模型走完管线。"""
from __future__ import annotations

import numpy as np
import pytest

from app import embed as embed_module


class _StubModel:
    """A deterministic replacement for SentenceTransformer used in tests."""

    def encode(self, texts, normalize_embeddings: bool = False):  # noqa: ARG002
        rng = np.random.default_rng(seed=42)
        return np.stack([rng.standard_normal(1024) for _ in texts])


@pytest.fixture(autouse=True)
def _stub_bge(monkeypatch):
    embed_module.reset_model_for_tests()
    monkeypatch.setattr(embed_module, "_get_model", lambda: _StubModel())
    yield
    embed_module.reset_model_for_tests()


def test_embed_two_texts_returns_two_vectors(client):
    r = client.post("/embed", json={"texts": ["第一句", "第二句"]})
    assert r.status_code == 200
    body = r.json()
    assert body["model"] == "BAAI/bge-m3"
    assert body["dim"] == 1024
    assert len(body["embeddings"]) == 2
    assert all(len(v) == 1024 for v in body["embeddings"])


def test_embed_empty_texts_rejected(client):
    r = client.post("/embed", json={"texts": []})
    # pydantic min_length=1 -> 422
    assert r.status_code == 422
