"""POST /ml/run: K-means and logistic regression via scikit-learn."""
from __future__ import annotations

from typing import Any

import numpy as np
from fastapi import APIRouter, HTTPException
from sklearn.cluster import KMeans
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import train_test_split

from app.schema import MlRequest, MlResponse


router = APIRouter()


@router.post("/ml/run", response_model=MlResponse)
def run(req: MlRequest) -> MlResponse:
    if req.task == "kmeans":
        return _kmeans(req)
    if req.task == "classify_logreg":
        return _logreg(req)
    raise HTTPException(status_code=400, detail=f"unknown task: {req.task}")


def _kmeans(req: MlRequest) -> MlResponse:
    n_clusters = int(req.params.get("n_clusters", 3))
    random_state = int(req.params.get("random_state", 42))
    if n_clusters < 1:
        raise HTTPException(status_code=400, detail="n_clusters must be >= 1")
    data = np.asarray(req.data, dtype=float)
    if data.ndim != 2:
        raise HTTPException(status_code=400, detail="data must be 2-D matrix")
    if data.shape[0] < n_clusters:
        raise HTTPException(
            status_code=400, detail=f"need >= {n_clusters} rows, got {data.shape[0]}"
        )
    try:
        km = KMeans(n_clusters=n_clusters, n_init="auto", random_state=random_state)
        labels = km.fit_predict(data)
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=500, detail=f"kmeans failed: {exc!s}") from exc
    return MlResponse(
        task="kmeans",
        result={
            "labels": labels.tolist(),
            "centroids": km.cluster_centers_.tolist(),
            "inertia": float(km.inertia_),
        },
    )


def _logreg(req: MlRequest) -> MlResponse:
    test_size = float(req.params.get("test_size", 0.2))
    random_state = int(req.params.get("random_state", 42))
    arr = np.asarray(req.data, dtype=float)
    if arr.ndim != 2 or arr.shape[1] < 2:
        raise HTTPException(
            status_code=400, detail="data must be 2-D with at least one feature column + label column"
        )
    x = arr[:, :-1]
    y = arr[:, -1].astype(int)
    if len(set(y.tolist())) < 2:
        raise HTTPException(status_code=400, detail="need at least 2 distinct labels")
    if arr.shape[0] < 4:
        raise HTTPException(status_code=400, detail="need >= 4 rows for split")

    try:
        x_train, x_test, y_train, y_test = train_test_split(
            x, y, test_size=test_size, random_state=random_state, stratify=y
        )
        clf = LogisticRegression(max_iter=200)
        clf.fit(x_train, y_train)
        preds: Any = clf.predict(x_test)
        acc = float((preds == y_test).mean())
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=500, detail=f"logreg failed: {exc!s}") from exc

    return MlResponse(
        task="classify_logreg",
        result={
            "accuracy": acc,
            "n_train": int(len(y_train)),
            "n_test": int(len(y_test)),
            "predictions": preds.tolist(),
        },
    )
