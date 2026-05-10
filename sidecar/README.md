# sqlai-sidecar

Python FastAPI sidecar serving BGE-M3 embeddings and sklearn ML tasks for the sqlai backend.

## Endpoints

- `GET /healthz` -> `{"ok": true}`
- `POST /embed` -> BGE-M3 vectors (1024-dim)
- `POST /ml/run` -> K-means / logistic regression tasks

## Local dev

```
cd sidecar
python -m venv .venv
.venv/Scripts/activate         # Windows
# . .venv/bin/activate         # *nix
pip install -e ".[dev]"
pytest
uvicorn app.main:app --host 0.0.0.0 --port 8081
```

## Embedding model

First request to `/embed` lazy-loads `BAAI/bge-m3` (~2.3 GB). Subsequent requests reuse the loaded instance.
