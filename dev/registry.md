# Forge Package Registry — 設計ドキュメント

> 関連: `crates/forge-cli/` — `forge add` / `forge publish` コマンド実装先
> 関連: `packages/anvil/` — Phase 1 API サーバーを Anvil で dogfooding

---

## 全体フェーズ

```
Phase 0: GitHub バックエンド   ← サーバー不要・今すぐ作れる
Phase 1: AWS + Terraform       ← スクショ取って close・将来への証拠
Phase 2: 本番運用              ← チーム・資金が整ったら
```

---

## Phase 0: GitHub バックエンド（サーバー不要）

### 設計思想

```
「forge add が動く」を最小コストで実現する。
サーバーなし・ホスティング費用ゼロ・今すぐ作れる。
```

Homebrew / Deno のサードパーティモジュールが最初にやった方法と同じ。

### リポジトリ構成

```
github.com/kazuma0606/forge-registry（新規リポジトリ）
  index.json              ← 全パッケージ一覧（GitHub Pages で配信）
  packages/
    forge/
      std-math.json       ← forge/std/math のメタデータ
      std-plot.json
    kazuma/
      my-lib.json         ← コミュニティパッケージ
  README.md
```

### index.json フォーマット

```jsonc
{
  "forge/std/math": {
    "description": "記号計算・微積分・線形代数",
    "latest": "0.1.0",
    "versions": {
      "0.1.0": {
        "url": "https://github.com/kazuma0606/rvm/releases/download/std-math-v0.1.0/std-math.tar.gz",
        "checksum": "sha256:abc123def456...",
        "forge_version": ">=0.1.0",
        "published_at": "2026-04-14"
      }
    }
  },
  "kazuma/my-lib": {
    "description": "My custom library",
    "latest": "1.2.0",
    "versions": {
      "1.2.0": {
        "url": "https://github.com/kazuma/my-lib/releases/download/v1.2.0/my-lib.tar.gz",
        "checksum": "sha256:xyz789...",
        "forge_version": ">=0.1.0",
        "published_at": "2026-04-14"
      }
    }
  }
}
```

### パッケージメタデータ（packages/forge/std-math.json）

```jsonc
{
  "name": "forge/std/math",
  "description": "記号計算・微積分・線形代数ライブラリ",
  "homepage": "https://github.com/kazuma0606/rvm/tree/master/lang/std/math",
  "license": "MIT",
  "authors": ["kazuma0606"],
  "versions": {
    "0.1.0": {
      "url": "https://github.com/kazuma0606/rvm/releases/download/std-math-v0.1.0/std-math.tar.gz",
      "checksum": "sha256:abc123...",
      "forge_version": ">=0.1.0",
      "dependencies": {},
      "published_at": "2026-04-14"
    }
  }
}
```

---

## forge.toml — パッケージ設定

```toml
[package]
name    = "my-app"
version = "0.1.0"
forge   = ">=0.1.0"

[dependencies]
"forge/std/math" = "0.1.0"
"forge/std/plot" = "0.1.0"
"kazuma/my-lib"  = "1.2.0"

[registry]
url = "https://kazuma0606.github.io/forge-registry/index.json"
# Phase 1 以降: url = "https://registry.forge-lang.dev/index.json"
```

---

## forge-cli コマンド設計

### `forge add <package>[@version]`

```bash
forge add forge/std/math           # latest
forge add forge/std/math@0.1.0     # バージョン指定
forge add kazuma/my-lib            # コミュニティパッケージ
```

**内部フロー：**
```
1. forge.toml を探す
2. registry.url から index.json を取得（キャッシュあり）
3. パッケージのメタデータ URL を解決
4. tar.gz をダウンロード
5. SHA256 チェックサム検証
6. ~/.forge/packages/<name>/<version>/ に展開
7. forge.toml の [dependencies] に追記
```

### `forge publish`

```bash
forge publish           # カレントパッケージを公開
forge publish --dry-run # 確認のみ
```

**内部フロー：**
```
1. forge.toml を読む
2. パッケージを tar.gz に圧縮
3. SHA256 チェックサムを計算
4. GitHub CLI (gh) で Release を作成・tar.gz をアップロード
5. forge-registry リポジトリの index.json / packages/*.json を PR で更新
   （自動: gh pr create）
6. マージされたらパッケージが利用可能になる
```

### `forge search <query>`

```bash
forge search math
# → forge/std/math   0.1.0  記号計算・微積分・線形代数
# → kazuma/mathutils 0.3.1  追加の数学ユーティリティ
```

### `forge info <package>`

```bash
forge info forge/std/math
# → Name:        forge/std/math
# → Description: 記号計算・微積分・線形代数ライブラリ
# → Latest:      0.1.0
# → License:     MIT
# → Downloads:   123
```

### `forge remove <package>`

```bash
forge remove kazuma/my-lib    # forge.toml から削除 + ローカルキャッシュ削除
```

---

## ローカルキャッシュ構造

```
~/.forge/
  packages/
    forge/
      std-math/
        0.1.0/
          forge.toml
          src/
            lib.forge
    kazuma/
      my-lib/
        1.2.0/
          ...
  registry/
    cache/
      index.json          ← TTL: 1時間
      forge/
        std-math.json
```

---

## Phase 1: AWS + Terraform（スクショ用）

### アーキテクチャ

```
forge add / forge publish
    ↓ HTTPS
CloudFront（CDN）
    ├── /packages/*  → S3（tar.gz ファイル）
    └── /api/*       → ECS（Anvil API サーバー）
                           ↓
                        RDS PostgreSQL（メタデータ）
```

### Terraform 構成

```
terraform/
  main.tf
  variables.tf
  outputs.tf
  modules/
    s3/           ← パッケージ本体ストレージ
    rds/          ← パッケージメタデータ DB
    ecs/          ← Anvil API サーバー
    cloudfront/   ← CDN + HTTPS
    route53/      ← registry.forge-lang.dev
```

### main.tf（骨格）

```hcl
terraform {
  required_providers {
    aws = { source = "hashicorp/aws", version = "~> 5.0" }
  }
}

provider "aws" {
  region = "ap-northeast-1"   # 東京リージョン
}

# パッケージ本体（tar.gz）
resource "aws_s3_bucket" "packages" {
  bucket = "forge-registry-packages-${var.env}"
}

resource "aws_s3_bucket_public_access_block" "packages" {
  bucket                  = aws_s3_bucket.packages.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# メタデータ DB
resource "aws_db_instance" "registry" {
  identifier        = "forge-registry-${var.env}"
  engine            = "postgres"
  engine_version    = "16"
  instance_class    = "db.t3.micro"   # 検証用最小
  allocated_storage = 20
  db_name           = "forge_registry"

  username = var.db_username
  password = var.db_password

  skip_final_snapshot = true   # destroy 時にスナップショット不要
  deletion_protection = false  # close しやすくする
}

# ECS クラスター（Anvil API）
resource "aws_ecs_cluster" "registry" {
  name = "forge-registry-${var.env}"
}

resource "aws_ecs_task_definition" "api" {
  family                   = "forge-registry-api"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = "256"   # 0.25 vCPU（最小）
  memory                   = "512"   # 512MB（最小）

  container_definitions = jsonencode([{
    name  = "api"
    image = "${var.ecr_image_url}:latest"
    portMappings = [{ containerPort = 8080 }]
    environment = [
      { name = "DATABASE_URL", value = local.db_url },
      { name = "S3_BUCKET",    value = aws_s3_bucket.packages.id },
    ]
  }])
}

# CloudFront
resource "aws_cloudfront_distribution" "registry" {
  enabled = true
  aliases = var.env == "prod" ? ["registry.forge-lang.dev"] : []

  origin {
    origin_id   = "s3-packages"
    domain_name = aws_s3_bucket.packages.bucket_regional_domain_name
    s3_origin_config { origin_access_identity = aws_cloudfront_origin_access_identity.main.cloudfront_access_identity_path }
  }

  origin {
    origin_id   = "api"
    domain_name = aws_lb.api.dns_name
    custom_origin_config {
      http_port  = 80
      https_port = 443
      origin_protocol_policy = "http-only"
    }
  }

  default_cache_behavior {
    target_origin_id = "s3-packages"
    viewer_protocol_policy = "redirect-to-https"
    allowed_methods  = ["GET", "HEAD"]
    cached_methods   = ["GET", "HEAD"]
  }

  ordered_cache_behavior {
    path_pattern     = "/api/*"
    target_origin_id = "api"
    viewer_protocol_policy = "redirect-to-https"
    allowed_methods  = ["GET", "HEAD", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"]
    cached_methods   = ["GET", "HEAD"]
  }

  restrictions { geo_restriction { restriction_type = "none" } }
  viewer_certificate { cloudfront_default_certificate = true }
}

output "registry_url" {
  value = "https://${aws_cloudfront_distribution.registry.domain_name}"
}
```

### API スキーマ（Anvil）

```
GET  /api/v1/packages              → パッケージ一覧
GET  /api/v1/packages/:name        → パッケージ詳細
GET  /api/v1/packages/:name/:ver   → バージョン詳細
POST /api/v1/packages              → パッケージ公開（認証必要）
GET  /api/v1/search?q=math         → 検索
GET  /api/v1/stats                 → ダウンロード統計
```

### 検証手順（スクショ用）

```bash
# 1. 立ち上げ（~5分）
cd terraform
terraform init
terraform apply -var env=demo

# 2. デモ
forge add forge/std/math            # インストール確認
curl https://<cloudfront>/api/v1/packages  # API 確認
# → ブラウザで Web UI を開いてスクショ

# 3. 即 close（数時間以内）
terraform destroy
# → 費用: 数十円
```

---

## Phase 2: 本番運用（チーム・資金が整ったら）

```
ドメイン: registry.forge-lang.dev
常時稼働: ECS Auto Scaling + RDS Multi-AZ
CDN:      CloudFront グローバルエッジ
監視:     CloudWatch + PagerDuty
CI/CD:    GitHub Actions → ECR → ECS デプロイ
認証:     GitHub OAuth（forge publish の認証）
Web UI:   forge-lang.dev/packages（Bloom で作る）
```

---

## 実装フェーズ

| フェーズ | 内容 | コスト | 期間 |
|---|---|---|---|
| **R-0** | forge-registry GitHub リポジトリ作成・index.json 設計 | ¥0 | 1日 |
| **R-1** | `forge add` コマンド実装（forge-cli）| ¥0 | 数日 |
| **R-2** | `forge publish` コマンド実装 | ¥0 | 数日 |
| **R-3** | `forge search` / `forge info` 実装 | ¥0 | 1日 |
| **R-4** | Terraform 作成 + AWS デモ起動 → スクショ → destroy | 数十円 | 半日 |
| **R-5** | 本番運用（チーム・資金が整ったら） | 月数千円〜 | — |

**R-3 完成 = README に「forge add で入れられます」と書ける**
**R-4 完成 = 「インフラ設計は済んでいる」という証拠**
