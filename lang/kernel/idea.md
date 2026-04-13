# forge-kernel — ForgeScript でカーネルを書く

> 関連: `lang/std/gpu/idea.md`（GPU ドライバ統合）
> 参考: [Writing an OS in Rust](https://os.phil-opp.com/) — Philipp Oppermann
> 参考: [Redox OS](https://www.redox-os.org/) — Rust 製 Unix 互換 OS

---

## 先に正直な話

**「tinyな Linuxカーネル」は作れるか？**

```
Linux カーネル本体の置き換え → 現実的でない（数百万行規模）
教育用・実験用の "toy OS kernel" → 作れる、面白い
Linux カーネルへの Rust モジュール追加 → 現実的（Linux 6.1 以降公式対応）
```

Rust コミュニティには既に実績がある：

| プロジェクト | 規模 | 状況 |
|---|---|---|
| [blog_os](https://os.phil-opp.com/) | 教育用 x86_64 OS | 最も参考になるチュートリアル |
| [Redox OS](https://redox-os.org/) | Unix 互換 OS | 本番稼働中 |
| [Asterinas](https://github.com/asterinas/asterinas) | Linux ABI 互換 | 研究用 |
| Linux kernel (Rust) | Linux 6.1〜 | ドライバを Rust で書ける |

**ForgeScript での現状：**
- 現在の forge-vm（インタープリタ）では **不可能**
- forge-transpiler（→ Rust → コンパイル）経由なら **設計次第で可能**

---

## なぜ Rust（とその上の ForgeScript）がカーネルに向くか

```
GC なし      → メモリ確保のタイミングが制御可能
所有権システム → use-after-free / double free がコンパイル時に防げる
unsafe 明示  → 危険な操作の箇所が一目でわかる
no_std 対応  → 標準ライブラリなしでビルドできる
inline asm   → x86 / RISC-V の命令を直接書ける
```

C でカーネルを書いたとき起きがちなバグの多くを型システムで防げる。

---

## ForgeScript がカーネルを書くために必要なもの

現在の ForgeScript に足りないピース：

### 1. `@no_std` モード

```forge
#![no_std]
#![no_main]

// 標準ライブラリなし、OS なし、ヒープなし
// → ベアメタル環境
```

### 2. `use raw {}` でインラインアセンブリ

```forge
// x86_64 のポート I/O（VGA テキスト出力、PIC 制御 etc.）
use raw {
    use core::arch::asm;
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value);
    }
}
```

### 3. ベアメタルビルドターゲット

```bash
forge build --target x86_64-unknown-none   # OS なし
forge build --target riscv64gc-unknown-none-elf
```

### 4. カスタムリンカスクリプト

```toml
# forge.toml
[kernel]
target    = "x86_64-unknown-none"
linker_script = "kernel.ld"
boot      = "multiboot2"    # multiboot2 / uefi / riscv-sbi
```

---

## 教育用カーネル（forge-kernel）の設計

Phil Oppermann の blog_os 相当を ForgeScript で書くとこうなる。

### ブート〜カーネルエントリ

```forge
// src/main.forge
#![no_std]
#![no_main]

use forge/kernel/vga.*
use forge/kernel/panic.*

// カーネルエントリポイント
@kernel_entry
fn kmain() -> ! {
    vga::print("Hello from ForgeScript Kernel!")
    loop {}
}

// パニックハンドラ（OS がないので自分で定義）
@panic_handler
fn on_panic(info: PanicInfo) -> ! {
    vga::print("KERNEL PANIC: {info.message()}")
    loop { halt() }
}
```

### VGA テキスト出力

```forge
// forge/kernel/vga.forge
// VGA テキストバッファ（物理アドレス 0xb8000）

const VGA_BUFFER: usize = 0xb8000
const WIDTH:  usize = 80
const HEIGHT: usize = 25

type ColorCode = u8   // 上位4bit: 背景色 / 下位4bit: 前景色

fn print_char(c: u8, color: ColorCode, col: usize, row: usize) {
    let offset = (row * WIDTH + col) * 2
    use raw {
        // volatile write（コンパイラの最適化で消されないように）
        let ptr = (VGA_BUFFER + offset) as *mut u8
        unsafe { ptr.write_volatile(c) }
        let ptr = (VGA_BUFFER + offset + 1) as *mut u8
        unsafe { ptr.write_volatile(color) }
    }
}

// マクロ風ラッパー
fn print(s: str) {
    for (i, c) in s.bytes().enumerate() {
        print_char(c, WHITE_ON_BLACK, i % WIDTH, i / WIDTH)
    }
}
```

### 物理メモリ管理（Bitmap Allocator）

```forge
// forge/kernel/pmm.forge

const PAGE_SIZE: usize = 4096

struct PhysMemManager {
    bitmap:     *mut u8,
    total_pages: usize,
    free_pages:  usize,
}

impl PhysMemManager {
    fn alloc_page() -> Option<usize> {
        for i in 0..self.total_pages {
            if !self.is_used(i) {
                self.set_used(i)
                self.free_pages -= 1
                return Some(i * PAGE_SIZE)
            }
        }
        None
    }

    fn free_page(addr: usize) {
        let page = addr / PAGE_SIZE
        self.set_free(page)
        self.free_pages += 1
    }
}
```

### 割り込みハンドラ（IDT）

```forge
// forge/kernel/idt.forge

// x86_64 割り込み記述子テーブル
struct Idt {
    entries: [IdtEntry; 256]
}

impl Idt {
    fn set_handler(vector: u8, handler: fn() -> !) {
        self.entries[vector] = IdtEntry::new(handler)
    }
}

// 割り込みハンドラ
@interrupt
fn double_fault(frame: InterruptStackFrame) -> ! {
    vga::print("EXCEPTION: DOUBLE FAULT\n{frame}")
    loop { halt() }
}

@interrupt
fn page_fault(frame: InterruptStackFrame, error: u64) {
    vga::print("PAGE FAULT: addr=0x{cr2():#x}, error={error}")
}

@interrupt
fn timer_interrupt(_frame: InterruptStackFrame) {
    TICK_COUNT += 1
    // PIC に EOI（End of Interrupt）を送る
    pic::send_eoi(0x20)
}
```

### 仮想メモリ（ページテーブル）

```forge
// forge/kernel/vmm.forge

// x86_64 の 4段ページテーブル（PML4 → PDP → PD → PT）
struct PageTable {
    entries: [PageTableEntry; 512]
}

fn map_page(virt: u64, phys: u64, flags: PageFlags) {
    let pml4 = active_pml4()
    let pdp  = ensure_table(pml4, virt_to_pml4_index(virt))
    let pd   = ensure_table(pdp,  virt_to_pdp_index(virt))
    let pt   = ensure_table(pd,   virt_to_pd_index(virt))
    pt[virt_to_pt_index(virt)] = PageTableEntry::new(phys, flags)
    tlb::flush(virt)
}

fn virt_to_phys(virt: u64) -> Option<u64> {
    // ページウォーク
    let pml4  = active_pml4()
    let entry = pml4[virt_to_pml4_index(virt)]
    // ... 4段辿る
}
```

---

## Linux カーネルモジュール（現実的なターゲット）

「ゼロから OS を作る」より現実的で実用的なアプローチ：

```
Linux 6.1 以降、ドライバを Rust で書ける
→ ForgeScript（→ Rust transpile）でドライバを書く
```

```forge
// my_driver.forge → forge-transpiler → Rust → Linux カーネルモジュール

use linux/kernel.*
use linux/module.*

@module(
    name: "forge_hello",
    author: "kazuma",
    description: "ForgeScript で書いた Linux カーネルモジュール",
    license: "GPL",
)
mod HelloModule {
    fn init() -> Result<()> {
        pr_info!("ForgeScript kernel module loaded!\n")
        Ok(())
    }

    fn exit() {
        pr_info!("ForgeScript kernel module unloaded!\n")
    }
}
```

```bash
# ビルド
forge build --target linux-module

# ロード（実際の Linux カーネルに）
sudo insmod forge_hello.ko
dmesg | tail        # → "ForgeScript kernel module loaded!"
sudo rmmod forge_hello
```

---

## 実装フェーズ

### Phase K-0: no_std 対応（transpiler）

| タスク | 内容 |
|---|---|
| `#![no_std]` モードの transpiler 対応 | |
| `@kernel_entry` `@panic_handler` アトリビュート | |
| `forge build --target x86_64-unknown-none` | |
| QEMU でカーネルイメージが起動すること | |

**マイルストーン: QEMU に "Hello from Forge!" が表示される**

### Phase K-1: ベアメタル基盤

| タスク | 内容 |
|---|---|
| VGA テキスト出力（`forge/kernel/vga`） | |
| シリアルポート出力（UART デバッグ用） | |
| `@interrupt` ハンドラ + IDT 初期化 | |
| タイマー割り込み（PIT）| |

### Phase K-2: メモリ管理

| タスク | 内容 |
|---|---|
| 物理メモリマネージャ（Bitmap Allocator） | |
| ページテーブル操作（x86_64 4段） | |
| カーネルヒープ（linked list allocator） | |
| `Box<T>` / `Vec<T>` が使えるようになる | |

### Phase K-3: プロセス管理

| タスク | 内容 |
|---|---|
| コンテキストスイッチ（レジスタ保存/復元） | |
| ラウンドロビンスケジューラ | |
| ユーザー空間 / カーネル空間の分離（リング0/3） | |

### Phase K-4: Linux カーネルモジュール

| タスク | 内容 |
|---|---|
| `forge build --target linux-module` | |
| `linux/kernel` バインディング | |
| `insmod` でロードできる最小モジュール | |
| キャラクターデバイスドライバのサンプル | |

---

## 難易度の正直な評価

```
VGA テキスト出力         ★☆☆  週末でできる
割り込みハンドラ          ★★☆  1〜2週間
ページテーブル / vmm      ★★★  数ヶ月
マルチプロセス / スケジューラ ★★★  半年以上
Linux カーネルモジュール   ★★☆  2〜4週間（現実的）
Redox OS 相当            ★★★  数年
```

**最も費用対効果が高いターゲット：**
1. **QEMU で動く toy OS**（K-0 〜 K-2）— 達成感があり学習価値が高い
2. **Linux カーネルモジュール**（K-4）— 実用的で RTX 5070 ドライバも書ける

---

## なぜやる価値があるか

```
ForgeScript がカーネルで動く
→ 「アプリからカーネルまで同じ言語で書ける」
→ Rust がそれを実現したように ForgeScript も狙える
→ 「ForgeScript は本物のシステム言語だ」という証明になる
```

教育的価値も大きく、**ノートブックで OS の仕組みを学ぶ** という
独自のユースケースも生まれる。

---

## 参考

- [Writing an OS in Rust](https://os.phil-opp.com/) — x86_64 OS を一から作るチュートリアル（最良の出発点）
- [Redox OS](https://www.redox-os.org/) — Rust 製 Unix 互換 OS
- [rCore-Tutorial](https://rcore-os.cn/rCore-Tutorial-Book-v3/) — RISC-V OS チュートリアル
- [Linux Rust bindings](https://rust.docs.kernel.org/) — Linux カーネル公式 Rust ドキュメント
- [Asterinas](https://github.com/asterinas/asterinas) — Linux ABI 互換 Rust カーネル
