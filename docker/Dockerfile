FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/root/.cargo/bin:${PATH}"

# 必要なビルドツールをインストール
RUN apt-get update -qq && \
    apt-get install -y --no-install-recommends \
        curl \
        ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Phase I-2: GitHub Releases バイナリを install.sh で取得
COPY install.sh /tmp/install.sh
RUN chmod +x /tmp/install.sh && FORGE_INSTALL_DIR=/usr/local/bin /tmp/install.sh

WORKDIR /workspace
CMD ["bash"]
