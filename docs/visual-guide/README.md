# agentprof 可视化指南

> 中文 HTML 教程，14 课分两章（用法 + Wiki），自包含可 `file://` 直开。

**📖 在线阅读**：<https://verdenmax.github.io/agentprof/>

## 本地构建

```
cargo run -p xtask -- visual-guide
open docs/visual-guide/index.html
```

## 子命令

- `cargo run -p xtask -- visual-guide`         — 生成
- `cargo run -p xtask -- visual-guide --clean` — 清空旧产物再生成
- `cargo run -p xtask -- visual-guide --check` — 仅校验（CI PR 路径，不写文件）

## 章节

- **用法**（6 课，面向新手）：what / install / analyze / list-aggregate / serve / db-otlp
- **Wiki**（8 课，面向中阶 + 开发者）：architecture / data-model / adapter / analyzer / storage / otlp / web-dashboard / contributing

## 文件布局

```
docs/visual-guide/
├── README.md       本文件（手工维护）
├── index.html      ⚙ 由 xtask 生成 — 入口
├── usage/          ⚙ 由 xtask 生成 — 6 课
├── wiki/           ⚙ 由 xtask 生成 — 8 课
└── assets/         ✋ 手工维护 — SVG / 截图（入 git）
```

生成的 `*.html` 不入 git（见 ADR-0025 D-2），只 commit 源码 + assets。

## 设计文档

- spec：`docs/superpowers/specs/2026-06-13-visual-guide-design.md`
- plan：`docs/superpowers/plans/2026-06-13-visual-guide.md`
- ADR：`docs/internals/adr-0025-visual-guide.md`（T21 落）
