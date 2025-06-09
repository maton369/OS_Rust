FROM debian:bookworm

# パッケージインストール
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    pkg-config \
    libssl-dev \
    sudo \
    clang \
    make \
    netcat-openbsd \
    qemu-system-x86 \
    ca-certificates \
    xz-utils \
    && rm -rf /var/lib/apt/lists/*

# 環境変数（PATH指定）
ENV RUSTUP_HOME=/root/.rustup \
    CARGO_HOME=/root/.cargo \
    PATH=/root/.cargo/bin:$PATH

# rustup + Rust nightly インストール
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly-2024-01-01

# 明示的にデフォルトツールチェーンを指定（念のため）
RUN rustup default nightly-2024-01-01 \
 && rustup component add rustfmt rust-src --toolchain nightly-2024-01-01

# バージョン確認
RUN rustup --version \
 && cargo --version \
 && rustc --version

WORKDIR /app

CMD ["bash"]
