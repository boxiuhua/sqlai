"""POST /embed: BGE-M3 lazy-loaded embeddings."""
from __future__ import annotations

import threading
from typing import TYPE_CHECKING

from fastapi import APIRouter, HTTPException

from app.schema import EmbedRequest, EmbedResponse

if TYPE_CHECKING:
    from sentence_transformers import SentenceTransformer


router = APIRouter()

_MODEL_NAME = "BAAI/bge-m3"
_EMBED_DIM = 1024
_model: "SentenceTransformer | None" = None
_lock = threading.Lock()


def _get_model() -> "SentenceTransformer":
    global _model
    if _model is not None:
        return _model
    with _lock:
        if _model is None:
            from sentence_transformers import SentenceTransformer

            _model = SentenceTransformer(_MODEL_NAME)
    return _model


def reset_model_for_tests() -> None:
    """Test-only hook to clear the lazy-loaded model."""
    global _model
    with _lock:
        _model = None


@router.post("/embed", response_model=EmbedResponse)
def embed(req: EmbedRequest) -> EmbedResponse:
    if not req.texts:
        raise HTTPException(status_code=400, detail="texts must be non-empty")
    try:
        model = _get_model()
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=503, detail=f"model unavailable: {exc!s}") from exc

    vectors = model.encode(req.texts, normalize_embeddings=True).tolist()
    return EmbedResponse(embeddings=vectors, model=_MODEL_NAME, dim=_EMBED_DIM)
