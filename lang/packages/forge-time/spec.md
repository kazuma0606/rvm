# `forge-time` 仕様書

> バージョン: 0.1.0
> 作成: 2026-04-08

---

## 概要

日時操作・タイムゾーン・継続時間を提供するパッケージ。
スケジューラ・ログのタイムスタンプ・期間計算に使用する。

---

## API

```forge
use forge_time.*

// 現在時刻
let now  = now()                              // DateTime
let ts   = timestamp()                        // number（Unix タイムスタンプ・秒）
let ts_ms = timestamp_ms()                   // number（ミリ秒）

// フォーマット
let s = format_date(now, "yyyy-MM-dd")        // "2026-04-08"
let s = format_date(now, "yyyy-MM-dd HH:mm:ss")

// パース
let dt = parse_date("2026-04-08", "yyyy-MM-dd")?   // DateTime!

// 演算
let later    = now.add_days(7)
let earlier  = now.sub_hours(3)
let diff     = dt2.diff(dt1)             // Duration

// Duration
let d = duration(1, "hour")             // 1時間
let d = duration(30, "minute")          // 30分
let d = duration(7, "day")              // 7日
// 単位: "second" / "minute" / "hour" / "day" / "week"

let secs = d.as_seconds()              // number
let mins = d.as_minutes()              // float
```

### DateTime フィールド

```forge
now.year        // number
now.month       // number（1〜12）
now.day         // number（1〜31）
now.hour        // number（0〜23）
now.minute      // number（0〜59）
now.second      // number（0〜59）
now.weekday     // string（"Monday" 〜 "Sunday"）
now.timezone    // string（"UTC" / "Asia/Tokyo" 等）
```

---

## Rust 変換

内部実装は `chrono` クレートを使用。
