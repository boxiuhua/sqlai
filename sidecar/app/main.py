"""FastAPI entrypoint."""
from __future__ import annotations

import os

from fastapi import FastAPI


def create_app() -> FastAPI:
    app = FastAPI(title="sqlai-sidecar", version="0.1.0")

    from app.embed import router as embed_router
    from app.ml import router as ml_router
    app.include_router(embed_router)
    app.include_router(ml_router)

    @app.get("/healthz")
    def healthz() -> dict[str, bool]:
        return {"ok": True}

    return app


app = create_app()


if os.environ.get("SIDECAR_PRELOAD_EMBED") == "1":
    # 占位；preload 实际接入后填充。
    pass
