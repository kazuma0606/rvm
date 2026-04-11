FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/root/.cargo/bin:${PATH}"

# 必要なビルドツールをインストール
RUN apt-get update -qq && \
    apt-get install -y --no-install-recommends \
        curl \
        ca-certificates \
        build-essential \
        git && \
    rm -rf /var/lib/apt/lists/*

# Rust インストール（非対話モード・stable）
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --no-modify-path

# forge インストール
# バイナリ名は forge-new のため forge としてシンボリックリンクを作成
RUN cargo install --git https://github.com/kazuma0606/rvm.git --bin forge-new --locked && \
    ln -s /root/.cargo/bin/forge-new /usr/local/bin/forge

# バージョン確認（ビルド時に動作確認）
RUN forge --version

WORKDIR /workspace
CMD ["bash"]
