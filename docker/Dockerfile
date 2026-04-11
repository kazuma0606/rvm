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

# forge インストール
# Phase I-2 (GitHub Releases 公開後):
#   RUN curl -sSf https://raw.githubusercontent.com/kazuma0606/rvm/master/install.sh | sh
#   → build-essential / Rust 不要になる
#
# Phase I-1 (現在): ソースからビルド
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --no-modify-path && \
    cargo install --git https://github.com/kazuma0606/rvm.git --bin forge-new --locked && \
    ln -s /root/.cargo/bin/forge-new /usr/local/bin/forge

WORKDIR /workspace
CMD ["bash"]
