// crucible-cli: PostgreSQL ORM and migration toolkit CLI
//
// コマンド:
//   crucible init [--psql]
//   crucible migrate
//   crucible migrate:rollback
//   crucible migrate:status
//   crucible make:migration <name>
//   crucible make:model <Name>
//   crucible make:repo <Name>
//   crucible schema:sync
//   crucible schema:diff

mod config;
mod init;
mod make;
mod migrate;
mod schema;

use std::env;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let command = &args[1];
    let result = match command.as_str() {
        "init" => {
            let use_psql = args.iter().any(|a| a == "--psql");
            init::run_init(use_psql)
        }
        "migrate" => match config::Config::load() {
            Ok(config) => migrate::run_migrate(&config).await,
            Err(e) => Err(e),
        },
        "migrate:rollback" => match config::Config::load() {
            Ok(config) => migrate::run_rollback(&config).await,
            Err(e) => Err(e),
        },
        "migrate:status" => match config::Config::load() {
            Ok(config) => migrate::run_status(&config).await,
            Err(e) => Err(e),
        },
        "make:migration" => {
            if args.len() < 3 {
                Err("使い方: crucible make:migration <name>".to_string())
            } else {
                make::run_make_migration(&args[2])
            }
        }
        "make:model" => {
            if args.len() < 3 {
                Err("使い方: crucible make:model <Name>".to_string())
            } else {
                make::run_make_model(&args[2])
            }
        }
        "make:repo" => {
            if args.len() < 3 {
                Err("使い方: crucible make:repo <Name>".to_string())
            } else {
                make::run_make_repo(&args[2])
            }
        }
        "schema:sync" => match config::Config::load() {
            Ok(config) => schema::run_schema_sync(&config).await,
            Err(e) => Err(e),
        },
        "schema:diff" => match config::Config::load() {
            Ok(config) => schema::run_schema_diff(&config).await,
            Err(e) => Err(e),
        },
        _ => {
            eprintln!("不明なコマンド: {}", command);
            print_usage();
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("エラー: {}", e);
        std::process::exit(1);
    }
}

fn print_usage() {
    println!("crucible — PostgreSQL ORM and migration toolkit");
    println!();
    println!("使い方:");
    println!("  crucible init [--psql]          プロジェクトを初期化する");
    println!("  crucible migrate                未適用のマイグレーションを適用する");
    println!("  crucible migrate:rollback       最後のマイグレーションを戻す");
    println!("  crucible migrate:status         マイグレーションの状態を表示する");
    println!("  crucible make:migration <name>  マイグレーションファイルを生成する");
    println!("  crucible make:model <Name>      モデルファイルを生成する");
    println!("  crucible make:repo <Name>       リポジトリファイルを生成する");
    println!("  crucible schema:sync            DB スキーマから schema.forge を生成する");
    println!("  crucible schema:diff            モデルと DB スキーマの差分を表示する");
}
