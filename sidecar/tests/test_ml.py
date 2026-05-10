def test_kmeans_clusters_well_separated_data(client):
    data = [[0.0, 0.0], [0.1, 0.0], [0.0, 0.1], [10.0, 10.0], [10.1, 10.0], [10.0, 10.1]]
    r = client.post("/ml/run", json={
        "task": "kmeans",
        "params": {"n_clusters": 2, "random_state": 0},
        "data": data,
    })
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["task"] == "kmeans"
    labels = body["result"]["labels"]
    assert labels[0] == labels[1] == labels[2]
    assert labels[3] == labels[4] == labels[5]
    assert labels[0] != labels[3]


def test_kmeans_too_few_rows_rejected(client):
    r = client.post("/ml/run", json={
        "task": "kmeans",
        "params": {"n_clusters": 3},
        "data": [[1.0], [2.0]],
    })
    assert r.status_code == 400


def test_classify_logreg_smoke(client):
    data = [
        [0.0, 0.0, 0],
        [0.1, 0.0, 0],
        [0.0, 0.1, 0],
        [0.05, 0.05, 0],
        [10.0, 10.0, 1],
        [10.1, 10.0, 1],
        [10.0, 10.1, 1],
        [10.05, 10.05, 1],
    ]
    r = client.post("/ml/run", json={
        "task": "classify_logreg",
        "params": {"test_size": 0.5, "random_state": 0},
        "data": data,
    })
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["task"] == "classify_logreg"
    assert 0.0 <= body["result"]["accuracy"] <= 1.0


def test_unknown_task_rejected(client):
    r = client.post("/ml/run", json={
        "task": "what",
        "params": {},
        "data": [[1.0]],
    })
    # pydantic Literal validation -> 422
    assert r.status_code == 422
