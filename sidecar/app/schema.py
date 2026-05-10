"""Pydantic v2 request/response schemas shared by all endpoints."""
from __future__ import annotations

from typing import Any, Literal
from pydantic import BaseModel, Field


class EmbedRequest(BaseModel):
    texts: list[str] = Field(min_length=1)


class EmbedResponse(BaseModel):
    embeddings: list[list[float]]
    model: str
    dim: int


class MlRequest(BaseModel):
    task: Literal["kmeans", "classify_logreg"]
    params: dict[str, Any] = Field(default_factory=dict)
    data: list[list[float]] = Field(min_length=1)


class MlResponse(BaseModel):
    task: str
    result: dict[str, Any]
