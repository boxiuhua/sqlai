# CPU-only torch；不带 CUDA，镜像更小。
# 使用 daocloud Docker Hub 镜像源（CN 友好）；境外用户可改回 `python:3.11-slim`。
FROM m.daocloud.io/docker.io/library/python:3.11-slim AS base

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PIP_NO_CACHE_DIR=1

WORKDIR /opt/sidecar

# 系统依赖：libgomp 是 sklearn / torch CPU 的运行时依赖。
RUN apt-get update \
 && apt-get install -y --no-install-recommends libgomp1 \
 && rm -rf /var/lib/apt/lists/*

# 先装依赖（利用 layer 缓存）
COPY pyproject.toml ./

# CN PyPI 镜像，加速主依赖安装
RUN pip install --upgrade pip -i https://pypi.tuna.tsinghua.edu.cn/simple

# torch CPU 单独一层：从 pytorch 官方 CPU index
RUN pip install --index-url https://download.pytorch.org/whl/cpu "torch>=2.3,<3"

# 其余主依赖从 PyPI 镜像装
RUN pip install -i https://pypi.tuna.tsinghua.edu.cn/simple \
      "fastapi>=0.115,<0.120" \
      "uvicorn[standard]>=0.30,<0.40" \
      "pydantic>=2.7,<3" \
      "numpy>=1.26,<3" \
      "scikit-learn>=1.5,<2" \
      "sentence-transformers>=3.0,<4"

COPY app ./app

EXPOSE 8081

CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8081"]
